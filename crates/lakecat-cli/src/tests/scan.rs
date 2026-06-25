use super::common::*;
use crate::*;

#[test]
fn qglake_scan_plan_verifier_requires_governed_projection() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let plan = PlanTableScanResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        status: "completed".to_string(),
        snapshot_id: None,
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest-list".to_string()],
        lakecat_plan_tasks: qglake_manifest_plan_tasks(),
        file_scan_tasks: Vec::new(),
        delete_files: Vec::new(),
        residual_filter: Some(serde_json::json!({
            "lakecat:scan-request": {
                "requested-projection": [
                    "event_id",
                    "occurred_at",
                    "severity",
                    "raw_payload"
                ],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "requested-stats-fields": ["event_id", "occurred_at", "severity", "raw_payload"],
                "effective-stats-fields": ["event_id", "occurred_at", "severity"],
                "stats-fields": ["event_id", "occurred_at", "severity"],
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                }
            }
        })),
    };

    verify_qglake_scan_plan(&plan).unwrap();
}

#[test]
fn qglake_scan_plan_verifier_rejects_missing_plan_task_token() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let plan = PlanTableScanResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        status: "completed".to_string(),
        snapshot_id: None,
        plan_tasks: Vec::new(),
        lakecat_plan_tasks: qglake_manifest_plan_tasks(),
        file_scan_tasks: Vec::new(),
        delete_files: Vec::new(),
        residual_filter: Some(serde_json::json!({
            "lakecat:scan-request": {
                "requested-projection": [
                    "event_id",
                    "occurred_at",
                    "severity",
                    "raw_payload"
                ],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "requested-stats-fields": ["event_id", "occurred_at", "severity", "raw_payload"],
                "effective-stats-fields": ["event_id", "occurred_at", "severity"],
                "stats-fields": ["event_id", "occurred_at", "severity"],
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                }
            }
        })),
    };

    let err = verify_qglake_scan_plan(&plan)
        .expect_err("QGLake governed scan should expose a plan-task token");
    assert!(err.to_string().contains("plan-task token"));
}

#[test]
fn qglake_scan_plan_verifier_rejects_missing_manifest_list_task() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let plan = PlanTableScanResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        status: "completed".to_string(),
        snapshot_id: None,
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest-list".to_string()],
        lakecat_plan_tasks: vec![serde_json::json!({"task-type": "metadata-only"})],
        file_scan_tasks: Vec::new(),
        delete_files: Vec::new(),
        residual_filter: Some(serde_json::json!({
            "lakecat:scan-request": {
                "requested-projection": [
                    "event_id",
                    "occurred_at",
                    "severity",
                    "raw_payload"
                ],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "requested-stats-fields": ["event_id", "occurred_at", "severity", "raw_payload"],
                "effective-stats-fields": ["event_id", "occurred_at", "severity"],
                "stats-fields": ["event_id", "occurred_at", "severity"],
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                }
            }
        })),
    };

    let err = verify_qglake_scan_plan(&plan)
        .expect_err("QGLake governed scan should expose a manifest-list task");
    assert!(err.to_string().contains("manifest-list task"));
}

#[test]
fn qglake_scan_plan_verifier_rejects_non_sail_planner() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let plan = PlanTableScanResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "memory-test-planner".to_string(),
        status: "completed".to_string(),
        snapshot_id: None,
        plan_tasks: Vec::new(),
        lakecat_plan_tasks: Vec::new(),
        file_scan_tasks: Vec::new(),
        delete_files: Vec::new(),
        residual_filter: Some(serde_json::json!({
            "lakecat:scan-request": {
                "requested-projection": [
                    "event_id",
                    "occurred_at",
                    "severity",
                    "raw_payload"
                ],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "requested-stats-fields": ["event_id", "occurred_at", "severity", "raw_payload"],
                "effective-stats-fields": ["event_id", "occurred_at", "severity"],
                "stats-fields": ["event_id", "occurred_at", "severity"],
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                }
            }
        })),
    };

    let err = verify_qglake_scan_plan(&plan)
        .expect_err("QGLake governed scan should require Sail planning");
    assert!(err.to_string().contains("not planned by Sail REST models"));
}

#[test]
fn qglake_scan_plan_verifier_requires_policy_hash_binding() {
    let plan = PlanTableScanResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        status: "completed".to_string(),
        snapshot_id: None,
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest-list".to_string()],
        lakecat_plan_tasks: qglake_manifest_plan_tasks(),
        file_scan_tasks: Vec::new(),
        delete_files: Vec::new(),
        residual_filter: Some(serde_json::json!({
            "lakecat:scan-request": {
                "requested-projection": [
                    "event_id",
                    "occurred_at",
                    "severity",
                    "raw_payload"
                ],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "requested-stats-fields": ["event_id", "occurred_at", "severity", "raw_payload"],
                "effective-stats-fields": ["event_id", "occurred_at", "severity"],
                "stats-fields": ["event_id", "occurred_at", "severity"],
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300
                }
            }
        })),
    };

    let err = verify_qglake_scan_plan(&plan)
        .expect_err("QGLake governed scan should require a policy hash binding");
    assert!(
        err.to_string()
            .contains("read restriction did not include policy hashes")
    );
}

#[test]
fn qglake_scan_plan_verifier_requires_read_restriction_purpose() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let plan = PlanTableScanResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        status: "completed".to_string(),
        snapshot_id: None,
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest-list".to_string()],
        lakecat_plan_tasks: qglake_manifest_plan_tasks(),
        file_scan_tasks: Vec::new(),
        delete_files: Vec::new(),
        residual_filter: Some(serde_json::json!({
            "lakecat:scan-request": {
                "requested-projection": [
                    "event_id",
                    "occurred_at",
                    "severity",
                    "raw_payload"
                ],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "requested-stats-fields": ["event_id", "occurred_at", "severity", "raw_payload"],
                "effective-stats-fields": ["event_id", "occurred_at", "severity"],
                "stats-fields": ["event_id", "occurred_at", "severity"],
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "policy-hashes": [expected_policy_hash]
                }
            }
        })),
    };

    let err = verify_qglake_scan_plan(&plan)
        .expect_err("QGLake governed scan should require read restriction purpose");
    assert!(err.to_string().contains("purpose"));
}

#[test]
fn qglake_scan_plan_verifier_requires_read_restriction_ttl_cap() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let plan = PlanTableScanResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        status: "completed".to_string(),
        snapshot_id: None,
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest-list".to_string()],
        lakecat_plan_tasks: qglake_manifest_plan_tasks(),
        file_scan_tasks: Vec::new(),
        delete_files: Vec::new(),
        residual_filter: Some(serde_json::json!({
            "lakecat:scan-request": {
                "requested-projection": [
                    "event_id",
                    "occurred_at",
                    "severity",
                    "raw_payload"
                ],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "requested-stats-fields": ["event_id", "occurred_at", "severity", "raw_payload"],
                "effective-stats-fields": ["event_id", "occurred_at", "severity"],
                "stats-fields": ["event_id", "occurred_at", "severity"],
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "policy-hashes": [expected_policy_hash]
                }
            }
        })),
    };

    let err = verify_qglake_scan_plan(&plan)
        .expect_err("QGLake governed scan should require read restriction TTL cap");
    assert!(err.to_string().contains("max credential TTL"));
}

#[test]
fn qglake_leaf_fetch_scan_tasks_verifier_accepts_terminal_manifest_expansion() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:manifest".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![serde_json::json!({
            "data-file": {
                "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
            }
        })],
        delete_files: Vec::new(),
        plan_tasks: Vec::new(),
        lakecat_plan_tasks: Vec::new(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    verify_qglake_leaf_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect("QGLake leaf manifest fetch should be terminal and governed");
}

#[test]
fn qglake_leaf_fetch_scan_tasks_verifier_rejects_more_child_tasks() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:manifest".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![serde_json::json!({
            "data-file": {
                "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
            }
        })],
        delete_files: Vec::new(),
        plan_tasks: vec!["lakecat:sail-json-hmac:unexpected".to_string()],
        lakecat_plan_tasks: Vec::new(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    let err = verify_qglake_leaf_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake leaf manifest fetch should be terminal");
    assert!(err.to_string().contains("unexpectedly exposed"));
}

#[test]
fn qglake_delete_manifest_fetch_scan_tasks_verifier_accepts_terminal_delete_work() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:delete-manifest".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: Vec::new(),
        delete_files: qglake_delete_files(),
        plan_tasks: Vec::new(),
        lakecat_plan_tasks: Vec::new(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    verify_qglake_delete_manifest_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect("QGLake delete manifest fetch should be terminal and governed");
}

#[test]
fn qglake_delete_manifest_fetch_scan_tasks_verifier_rejects_escaped_delete_files() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:delete-manifest".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: Vec::new(),
        delete_files: vec![serde_json::json!({
            "content": "position-deletes",
            "file-path": "file:///tmp/lakecat-qglake/other/delete/pos-delete-1.parquet"
        })],
        plan_tasks: Vec::new(),
        lakecat_plan_tasks: Vec::new(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    let err = verify_qglake_delete_manifest_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake delete manifest fetch should reject escaped delete files");
    assert!(
        err.to_string()
            .contains("delete file escaped table location")
    );
}

#[test]
fn qglake_fetch_scan_tasks_verifier_requires_reapplied_policy_hash_binding() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![qglake_file_scan_task_with_delete_ref()],
        delete_files: qglake_delete_files(),
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
        lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION).unwrap();
}

#[test]
fn qglake_fetch_scan_tasks_verifier_requires_effective_projection() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![qglake_file_scan_task_with_delete_ref()],
        delete_files: qglake_delete_files(),
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
        lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake governed fetch should require effective projection proof");
    assert!(err.to_string().contains("effective projection"));
}

#[test]
fn qglake_fetch_scan_tasks_verifier_requires_delete_file_refs() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![serde_json::json!({
            "data-file": {
                "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
            }
        })],
        delete_files: qglake_delete_files(),
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
        lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake governed fetch should require delete-file references");
    assert!(err.to_string().contains("delete-file references"));
}

#[test]
fn qglake_fetch_scan_tasks_verifier_requires_delete_files() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![qglake_file_scan_task_with_delete_ref()],
        delete_files: Vec::new(),
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
        lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake governed fetch should require delete-file entries");
    assert!(err.to_string().contains("delete-file refs"));
}

#[test]
fn qglake_fetch_scan_tasks_verifier_accepts_multiple_manifest_children() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![qglake_file_scan_task_with_delete_ref()],
        delete_files: qglake_delete_files(),
        plan_tasks: vec![
            "lakecat:sail-json-hmac:manifest:1".to_string(),
            "lakecat:sail-json-hmac:manifest:2".to_string(),
        ],
        lakecat_plan_tasks: vec![
            serde_json::json!({
                "task-type": "manifest",
                "manifest-list": "file:///tmp/lakecat-qglake/events/metadata/snap-42.avro",
                "manifest-path": "file:///tmp/lakecat-qglake/events/metadata/manifest-42-a.avro",
                "content": "data"
            }),
            serde_json::json!({
                "task-type": "manifest",
                "manifest-list": "file:///tmp/lakecat-qglake/events/metadata/snap-42.avro",
                "manifest-path": "file:///tmp/lakecat-qglake/events/metadata/delete-manifest-42.avro",
                "content": "deletes"
            }),
        ],
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect("QGLake manifest-list fetch should accept multiple child manifests");
}

#[test]
fn qglake_fetch_scan_tasks_verifier_rejects_missing_child_plan_task_token() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![serde_json::json!({
            "data-file": {
                "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
            }
        })],
        delete_files: Vec::new(),
        plan_tasks: Vec::new(),
        lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake governed fetch should expose child plan-task tokens");
    assert!(
        err.to_string()
            .contains("child Iceberg REST plan-task token")
    );
}

#[test]
fn qglake_fetch_scan_tasks_verifier_rejects_missing_manifest_child_task() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![serde_json::json!({
            "data-file": {
                "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
            }
        })],
        delete_files: Vec::new(),
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
        lakecat_plan_tasks: vec![serde_json::json!({"task-type": "metadata-only"})],
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake governed fetch should expose manifest child tasks");
    assert!(err.to_string().contains("manifest child task"));
}

#[test]
fn qglake_fetch_scan_tasks_verifier_rejects_non_sail_planner() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "memory-test-planner".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![serde_json::json!({
            "data-file": {
                "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
            }
        })],
        delete_files: Vec::new(),
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
        lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake governed fetch should require Sail planning");
    assert!(err.to_string().contains("not planned by Sail REST models"));
}

#[test]
fn qglake_fetch_scan_tasks_verifier_rejects_empty_scan_work() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: Vec::new(),
        delete_files: Vec::new(),
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
        lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake governed fetch should require scan work");
    assert!(err.to_string().contains("produced no file scan tasks"));
}

#[test]
fn qglake_fetch_scan_tasks_verifier_rejects_placeholder_scan_work() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![serde_json::json!({"placeholder": true})],
        delete_files: Vec::new(),
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
        lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake governed fetch should require data-file paths");
    assert!(err.to_string().contains("no data-file file paths"));
}

#[test]
fn qglake_fetch_scan_tasks_verifier_rejects_escaped_data_file_paths() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![serde_json::json!({
            "data-file": {
                "file-path": "file:///tmp/lakecat-qglake/other-table/data/part-1.parquet"
            }
        })],
        delete_files: Vec::new(),
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
        lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake governed fetch should reject escaped data files");
    assert!(err.to_string().contains("escaped table location"));
}

#[test]
fn qglake_fetch_scan_tasks_verifier_rejects_widened_allowed_columns() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![serde_json::json!({
            "data-file": {
                "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
            }
        })],
        delete_files: Vec::new(),
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
        lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": [
                        "event_id",
                        "occurred_at",
                        "severity",
                        "raw_payload"
                    ],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake governed fetch should reject widened columns");
    assert!(err.to_string().contains("allowed columns"));
}

#[test]
fn qglake_fetch_scan_tasks_verifier_rejects_missing_policy_hash_binding() {
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![serde_json::json!({
            "data-file": {
                "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
            }
        })],
        delete_files: Vec::new(),
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
        lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake governed fetch should require a policy hash binding");
    assert!(
        err.to_string()
            .contains("read restriction did not include policy hashes")
    );
}

#[test]
fn qglake_fetch_scan_tasks_verifier_requires_read_restriction_purpose() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![qglake_file_scan_task_with_delete_ref()],
        delete_files: qglake_delete_files(),
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
        lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake governed fetch should require read restriction purpose");
    assert!(err.to_string().contains("purpose"));
}

#[test]
fn qglake_fetch_scan_tasks_verifier_requires_read_restriction_ttl_cap() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![qglake_file_scan_task_with_delete_ref()],
        delete_files: qglake_delete_files(),
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
        lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake governed fetch should require read restriction TTL cap");
    assert!(err.to_string().contains("max credential TTL"));
}

#[test]
fn qglake_fetch_scan_tasks_verifier_rejects_missing_required_projection() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![qglake_file_scan_task_with_delete_ref()],
        delete_files: qglake_delete_files(),
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
        lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            }
        })),
    };

    let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake governed fetch should require required projection evidence");
    assert!(err.to_string().contains("required projection"));
}

#[test]
fn qglake_fetch_scan_tasks_verifier_rejects_missing_required_filters() {
    let expected_policy_hash = qglake_policy_hash("events").unwrap();
    let fetched = FetchScanTasksResponse {
        table: lakecat_api::TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        planned_by: "sail-rest-models".to_string(),
        plan_task: "lakecat:sail-json-hmac:test".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![qglake_file_scan_task_with_delete_ref()],
        delete_files: qglake_delete_files(),
        plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
        lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
        residual_filter: Some(serde_json::json!({
            "lakecat:fetch-scan-tasks": {
                "read-restriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [expected_policy_hash]
                },
                "required-projection": ["event_id", "occurred_at", "severity"],
                "effective-projection": ["event_id", "occurred_at", "severity"]
            }
        })),
    };

    let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
        .expect_err("QGLake governed fetch should require row-predicate proof");
    assert!(err.to_string().contains("required filters"));
}

#[test]
fn qglake_scan_replay_line_summarizes_verified_evidence() {
    let line = qglake_scan_replay_line(&LineageDrainResponse {
        delivered: 2,
        event_types: vec![
            "table.scan-planned".to_string(),
            "table.scan-tasks-fetched".to_string(),
        ],
        graph_events: 1,
        lineage_events: 2,
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
        authorization_receipt_action: Some("lineage-read".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        events: vec![
            qglake_scan_planned_lineage_summary(),
            qglake_scan_tasks_fetched_lineage_summary(),
        ],
    })
    .expect("scan replay line should be present");

    assert_eq!(
        line,
        "scan replay plan_tasks=1 plan_graph_events=1 planned_ttl=300 planned_purpose=qglake-agent-demo file_tasks=1 delete_files=1 child_plan_tasks=1 fetched_ttl=300 fetched_purpose=qglake-agent-demo"
    );
}

#[test]
fn qglake_scan_replay_rejects_missing_stats_field_evidence() {
    let mut planned = qglake_scan_planned_lineage_summary();
    planned.requested_stats_fields = Vec::new();

    let err = verify_qglake_scan_restriction_replay(
        &planned,
        &qglake_scan_tasks_fetched_lineage_summary(),
    )
    .expect_err("scan replay should reject missing stats-field evidence");

    assert!(
        err.to_string()
            .contains("missing requested/effective stats-field evidence")
    );
}

#[test]
fn qglake_scan_replay_rejects_missing_projection_evidence() {
    let mut planned = qglake_scan_planned_lineage_summary();
    planned.requested_projection = Vec::new();

    let err = verify_qglake_scan_restriction_replay(
        &planned,
        &qglake_scan_tasks_fetched_lineage_summary(),
    )
    .expect_err("scan replay should reject missing projection evidence");

    assert!(
        err.to_string()
            .contains("missing requested/effective projection evidence")
    );
}

#[test]
fn qglake_scan_replay_rejects_widened_effective_projection() {
    let mut planned = qglake_scan_planned_lineage_summary();
    planned.effective_projection = vec![
        "event_id".to_string(),
        "occurred_at".to_string(),
        "severity".to_string(),
        "raw_payload".to_string(),
    ];

    let err = verify_qglake_scan_restriction_replay(
        &planned,
        &qglake_scan_tasks_fetched_lineage_summary(),
    )
    .expect_err("scan replay should reject widened effective projection");

    assert!(
        err.to_string()
            .contains("does not prove projection narrowing")
    );
}

#[test]
fn qglake_scan_replay_rejects_unrequested_effective_projection() {
    let mut planned = qglake_scan_planned_lineage_summary();
    planned.requested_projection = vec![
        "event_id".to_string(),
        "occurred_at".to_string(),
        "severity".to_string(),
        "raw_payload".to_string(),
    ];
    planned.effective_projection = vec![
        "event_id".to_string(),
        "occurred_at".to_string(),
        "tenant_id".to_string(),
    ];

    let err = verify_qglake_scan_restriction_replay(
        &planned,
        &qglake_scan_tasks_fetched_lineage_summary(),
    )
    .expect_err("scan replay should reject effective projection fields that were never requested");

    assert!(err.to_string().contains("tenant_id"));
    assert!(err.to_string().contains("was not requested"));
}

#[test]
fn qglake_scan_replay_rejects_duplicate_requested_projection() {
    let mut planned = qglake_scan_planned_lineage_summary();
    planned.requested_projection.push("raw_payload".to_string());

    let err = verify_qglake_scan_restriction_replay(
        &planned,
        &qglake_scan_tasks_fetched_lineage_summary(),
    )
    .expect_err("scan replay should reject duplicate requested projection evidence");

    assert!(
        err.to_string()
            .contains("scan planning requested projection")
    );
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_scan_replay_rejects_missing_fetched_effective_projection() {
    let mut fetched = qglake_scan_tasks_fetched_lineage_summary();
    fetched.effective_projection = Vec::new();

    let err =
        verify_qglake_scan_restriction_replay(&qglake_scan_planned_lineage_summary(), &fetched)
            .expect_err("scan replay should reject missing fetched effective projection");

    assert!(
        err.to_string()
            .contains("fetch replay effective projection does not match")
    );
}

#[test]
fn qglake_scan_replay_rejects_drifted_fetched_effective_projection() {
    let mut fetched = qglake_scan_tasks_fetched_lineage_summary();
    fetched.effective_projection = vec![
        "event_id".to_string(),
        "occurred_at".to_string(),
        "raw_payload".to_string(),
    ];

    let err =
        verify_qglake_scan_restriction_replay(&qglake_scan_planned_lineage_summary(), &fetched)
            .expect_err("scan replay should reject drifted fetched effective projection");

    assert!(
        err.to_string()
            .contains("fetch replay effective projection does not match")
    );
}

#[test]
fn qglake_scan_replay_rejects_widened_effective_stats_fields() {
    let mut planned = qglake_scan_planned_lineage_summary();
    planned.effective_stats_fields = vec![
        "event_id".to_string(),
        "occurred_at".to_string(),
        "severity".to_string(),
        "raw_payload".to_string(),
    ];

    let err = verify_qglake_scan_restriction_replay(
        &planned,
        &qglake_scan_tasks_fetched_lineage_summary(),
    )
    .expect_err("scan replay should reject widened effective stats fields");

    assert!(
        err.to_string()
            .contains("does not prove stats-field narrowing")
    );
}

#[test]
fn qglake_scan_replay_rejects_unrequested_effective_stats_fields() {
    let mut planned = qglake_scan_planned_lineage_summary();
    planned.requested_stats_fields = vec![
        "event_id".to_string(),
        "occurred_at".to_string(),
        "severity".to_string(),
        "raw_payload".to_string(),
    ];
    planned.effective_stats_fields = vec![
        "event_id".to_string(),
        "occurred_at".to_string(),
        "tenant_id".to_string(),
    ];

    let err = verify_qglake_scan_restriction_replay(
        &planned,
        &qglake_scan_tasks_fetched_lineage_summary(),
    )
    .expect_err("scan replay should reject effective stats fields that were never requested");

    assert!(err.to_string().contains("tenant_id"));
    assert!(err.to_string().contains("was not requested"));
}

#[test]
fn qglake_scan_replay_rejects_blank_requested_stats_field() {
    let mut planned = qglake_scan_planned_lineage_summary();
    planned.requested_stats_fields.push(" ".to_string());

    let err = verify_qglake_scan_restriction_replay(
        &planned,
        &qglake_scan_tasks_fetched_lineage_summary(),
    )
    .expect_err("scan replay should reject blank requested stats-field evidence");

    assert!(
        err.to_string()
            .contains("scan planning requested stats fields")
    );
    assert!(err.to_string().contains("non-empty"));
}
