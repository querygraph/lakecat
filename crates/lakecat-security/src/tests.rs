use lakecat_core::{Namespace, PrincipalKind, TableName, WarehouseName};

use super::*;

#[test]
fn read_restriction_composes_odrl_policy_documents() {
    let policy_a = serde_json::json!({
        "uid": "policy-a",
        "purpose": "resilience-demo",
        "lakecat:read-restriction": {
            "max-credential-ttl-seconds": 900,
            "allowed-columns": ["event_id", "payload"],
            "row-predicate": {
                "type": "equal",
                "term": "region",
                "value": "west"
            }
        }
    });
    let policy_b = serde_json::json!({
        "uid": "policy-b",
        "max-credential-ttl-seconds": 300,
        "permission": [{
            "constraint": [
                {
                    "leftOperand": "allowed-columns",
                    "operator": "eq",
                    "rightOperand": ["event_id", "severity"]
                },
                {
                    "leftOperand": "row-predicate",
                    "operator": "eq",
                    "rightOperand": {
                        "type": "greater-than-or-equal",
                        "term": "severity",
                        "value": 3
                    }
                }
            ]
        }]
    });

    let restriction = ReadRestriction::from_odrl_policies([&policy_a, &policy_b]).unwrap();

    assert_eq!(
        restriction.allowed_columns,
        Some(vec!["event_id".to_string()])
    );
    assert_eq!(restriction.purpose.as_deref(), Some("resilience-demo"));
    assert_eq!(restriction.max_credential_ttl_seconds, Some(300));
    assert_eq!(restriction.policy_hashes.len(), 2);
    assert_eq!(
        restriction.row_predicate,
        Some(serde_json::json!({
            "type": "and",
            "left": {
                "type": "equal",
                "term": "region",
                "value": "west"
            },
            "right": {
                "type": "greater-than-or-equal",
                "term": "severity",
                "value": 3
            }
        }))
    );
}

#[test]
fn read_restriction_parses_ttl_from_odrl_constraints_and_uses_tightest_ttl() {
    let policy_a = serde_json::json!({
        "uid": "policy-a",
        "lakecat:read-restriction": {
            "max-credential-ttl-seconds": 900
        }
    });
    let policy_b = serde_json::json!({
        "uid": "policy-b",
        "permission": [{
            "constraint": {
                "leftOperand": "max-credential-ttl-seconds",
                "operator": "lteq",
                "rightOperand": 300
            }
        }]
    });

    let restriction = ReadRestriction::from_odrl_policies([&policy_a, &policy_b]).unwrap();

    assert_eq!(restriction.max_credential_ttl_seconds, Some(300));
}

#[test]
fn read_restriction_uses_tightest_ttl_within_policy_document() {
    let policy = serde_json::json!({
        "uid": "policy-a",
        "max-credential-ttl-seconds": 900,
        "lakecat:read-restriction": {
            "max-credential-ttl-seconds": 600
        },
        "permission": [{
            "constraint": [
                {
                    "leftOperand": "max-credential-ttl-seconds",
                    "operator": "lteq",
                    "rightOperand": 300
                },
                {
                    "leftOperand": "credential-ttl",
                    "operator": "eq",
                    "rightOperand": 1200
                }
            ]
        }]
    });

    let restriction = ReadRestriction::from_odrl_policies([&policy]).unwrap();

    assert_eq!(restriction.max_credential_ttl_seconds, Some(300));
}

#[test]
fn read_restriction_accepts_prefixed_odrl_constraint_operands() {
    let policy = serde_json::json!({
        "uid": "policy-a",
        "permission": [{
            "constraint": [
                {
                    "odrl:leftOperand": "allowed-columns",
                    "odrl:operator": "odrl:eq",
                    "odrl:rightOperand": ["event_id", "severity"]
                },
                {
                    "odrl:leftOperand": "row-predicate",
                    "odrl:operator": "http://www.w3.org/ns/odrl/2/eq",
                    "odrl:rightOperand": {
                        "type": "equal",
                        "term": "region",
                        "value": "west"
                    }
                },
                {
                    "odrl:leftOperand": "purpose",
                    "odrl:operator": "odrl:eq",
                    "odrl:rightOperand": "resilience-demo"
                },
                {
                    "odrl:leftOperand": "credential-ttl",
                    "odrl:operator": "odrl:lteq",
                    "odrl:rightOperand": 300
                }
            ]
        }]
    });

    let restriction = ReadRestriction::from_odrl_policies([&policy]).unwrap();

    assert_eq!(
        restriction.allowed_columns,
        Some(vec!["event_id".to_string(), "severity".to_string()])
    );
    assert_eq!(
        restriction.row_predicate,
        Some(serde_json::json!({
            "type": "equal",
            "term": "region",
            "value": "west"
        }))
    );
    assert_eq!(restriction.purpose.as_deref(), Some("resilience-demo"));
    assert_eq!(restriction.max_credential_ttl_seconds, Some(300));
}

#[test]
fn read_restriction_accepts_jsonld_term_objects_for_constraint_terms() {
    let policy = serde_json::json!({
        "uid": "policy-a",
        "permission": [{
            "constraint": [
                {
                    "leftOperand": { "@id": "lakecat:allowed-columns" },
                    "operator": { "@id": "odrl:eq" },
                    "rightOperand": ["event_id"]
                },
                {
                    "odrl:leftOperand": { "@id": "lakecat:row-predicate" },
                    "odrl:operator": { "@id": "http://www.w3.org/ns/odrl/2/eq" },
                    "odrl:rightOperand": {
                        "type": "equal",
                        "term": "region",
                        "value": "west"
                    }
                }
            ]
        }]
    });

    let restriction = ReadRestriction::from_odrl_policies([&policy]).unwrap();

    assert_eq!(
        restriction.allowed_columns,
        Some(vec!["event_id".to_string()])
    );
    assert_eq!(
        restriction.row_predicate,
        Some(serde_json::json!({
            "type": "equal",
            "term": "region",
            "value": "west"
        }))
    );
}

#[test]
fn read_restriction_accepts_jsonld_value_objects_for_right_operands() {
    let policy = serde_json::json!({
        "uid": "policy-a",
        "purpose": { "@value": "resilience-demo" },
        "permission": [{
            "constraint": [
                {
                    "leftOperand": { "@id": "lakecat:allowed-columns" },
                    "operator": { "@id": "odrl:isAnyOf" },
                    "rightOperand": {
                        "@list": [
                            { "@value": "event_id" },
                            { "@value": "severity" },
                            { "@value": "event_id" }
                        ]
                    }
                },
                {
                    "leftOperand": { "@id": "lakecat:purpose" },
                    "operator": { "@id": "odrl:eq" },
                    "rightOperand": { "@value": "resilience-demo" }
                },
                {
                    "leftOperand": { "@id": "lakecat:credential-ttl" },
                    "operator": { "@id": "odrl:lteq" },
                    "rightOperand": {
                        "@value": "300",
                        "@type": "http://www.w3.org/2001/XMLSchema#unsignedLong"
                    }
                }
            ]
        }]
    });

    let restriction = ReadRestriction::from_odrl_policies([&policy]).unwrap();

    assert_eq!(
        restriction.allowed_columns,
        Some(vec!["event_id".to_string(), "severity".to_string()])
    );
    assert_eq!(restriction.purpose.as_deref(), Some("resilience-demo"));
    assert_eq!(restriction.max_credential_ttl_seconds, Some(300));
}

#[test]
fn read_restriction_rejects_malformed_jsonld_allowed_column_lists() {
    let policy = serde_json::json!({
        "uid": "policy-a",
        "permission": [{
            "constraint": {
                "leftOperand": { "@id": "lakecat:allowed-columns" },
                "operator": { "@id": "odrl:isAnyOf" },
                "rightOperand": {
                    "@list": [
                        { "@value": "event_id" },
                        { "@id": "lakecat:not-a-column-value" }
                    ]
                }
            }
        }]
    });

    let err = ReadRestriction::from_odrl_policies([&policy]).unwrap_err();

    assert!(
        err.to_string()
            .contains("ODRL allowed columns must be strings")
    );
}

#[test]
fn read_restriction_rejects_empty_or_blank_allowed_column_lists() {
    for policy in [
        serde_json::json!({
            "uid": "policy-a",
            "lakecat:read-restriction": {
                "allowed-columns": []
            }
        }),
        serde_json::json!({
            "uid": "policy-b",
            "lakecat:read-restriction": {
                "allowed-columns": ["event_id", " "]
            }
        }),
        serde_json::json!({
            "uid": "policy-c",
            "permission": [{
                "constraint": {
                    "leftOperand": { "@id": "lakecat:allowed-columns" },
                    "operator": { "@id": "odrl:isAnyOf" },
                    "rightOperand": {
                        "@list": [
                            { "@value": "event_id" },
                            { "@value": "" }
                        ]
                    }
                }
            }]
        }),
    ] {
        let err = ReadRestriction::from_odrl_policies([&policy]).unwrap_err();

        assert!(
            err.to_string().contains("ODRL allowed columns must not be"),
            "unexpected error: {err}"
        );
    }
}

#[test]
fn read_restriction_rejects_unsupported_odrl_constraint_operators() {
    let policy = serde_json::json!({
        "permission": [{
            "constraint": {
                "leftOperand": "allowed-columns",
                "operator": "neq",
                "rightOperand": ["secret_payload"]
            }
        }]
    });

    let err = ReadRestriction::from_odrl_policies([&policy]).unwrap_err();

    assert!(
        err.to_string()
            .contains("ODRL allowed columns constraint uses unsupported operator")
    );
}

#[test]
fn read_restriction_rejects_missing_odrl_constraint_operator() {
    let policy = serde_json::json!({
        "permission": [{
            "constraint": {
                "leftOperand": "row-predicate",
                "rightOperand": {
                    "type": "equal",
                    "term": "region",
                    "value": "west"
                }
            }
        }]
    });

    let err = ReadRestriction::from_odrl_policies([&policy]).unwrap_err();

    assert!(
        err.to_string()
            .contains("ODRL row predicate constraint must include an operator")
    );
}

#[test]
fn read_restriction_rejects_missing_odrl_constraint_right_operands() {
    for (left_operand_key, left_operand, label) in [
        ("leftOperand", "allowed-columns", "allowed columns"),
        ("leftOperand", "row-predicate", "row predicate"),
        ("leftOperand", "purpose", "purpose"),
        ("leftOperand", "credential-ttl", "max credential TTL"),
        ("odrl:leftOperand", "allowed-columns", "allowed columns"),
    ] {
        let mut constraint = serde_json::Map::new();
        constraint.insert(
            left_operand_key.to_string(),
            serde_json::Value::String(left_operand.to_string()),
        );
        constraint.insert(
            "operator".to_string(),
            serde_json::Value::String("eq".to_string()),
        );
        let policy = serde_json::json!({
            "permission": [{
                "constraint": constraint
            }]
        });

        let err = ReadRestriction::from_odrl_policies([&policy]).unwrap_err();

        assert!(
            err.to_string().contains(&format!(
                "ODRL {label} constraint must include a right operand"
            )),
            "unexpected error for {left_operand}: {err}"
        );
    }
}

#[test]
fn read_restriction_rejects_non_equality_purpose_constraint() {
    let policy = serde_json::json!({
        "permission": [{
            "constraint": {
                "leftOperand": "purpose",
                "operator": "neq",
                "rightOperand": "resilience-demo"
            }
        }]
    });

    let err = ReadRestriction::from_odrl_policies([&policy]).unwrap_err();

    assert!(
        err.to_string()
            .contains("ODRL purpose constraint uses unsupported operator")
    );
}

#[test]
fn read_restriction_rejects_blank_odrl_purposes() {
    for policy in [
        serde_json::json!({
            "uid": "policy-a",
            "purpose": " "
        }),
        serde_json::json!({
            "uid": "policy-b",
            "permission": [{
                "constraint": {
                    "leftOperand": "purpose",
                    "operator": "eq",
                    "rightOperand": { "@value": "" }
                }
            }]
        }),
    ] {
        let err = ReadRestriction::from_odrl_policies([&policy]).unwrap_err();

        assert!(
            err.to_string().contains("ODRL purpose")
                && err.to_string().contains("must not be blank"),
            "unexpected error: {err}"
        );
    }
}

#[test]
fn read_restriction_rejects_conflicting_purpose_constraints() {
    let policy = serde_json::json!({
        "purpose": "resilience-demo",
        "permission": [{
            "constraint": {
                "leftOperand": "purpose",
                "operator": "eq",
                "rightOperand": "training"
            }
        }]
    });

    let err = ReadRestriction::from_odrl_policies([&policy]).unwrap_err();

    assert!(
        err.to_string()
            .contains("ODRL read restriction carries conflicting purposes")
    );
}

#[test]
fn read_restriction_rejects_conflicting_policy_purposes() {
    let policy_a = serde_json::json!({
        "uid": "policy-a",
        "purpose": "resilience-demo"
    });
    let policy_b = serde_json::json!({
        "uid": "policy-b",
        "permission": [{
            "constraint": {
                "leftOperand": "purpose",
                "operator": "eq",
                "rightOperand": "training"
            }
        }]
    });

    let err = ReadRestriction::from_odrl_policies([&policy_a, &policy_b]).unwrap_err();

    assert!(
        err.to_string()
            .contains("ODRL read restriction carries conflicting purposes")
    );
}

#[test]
fn read_restriction_rejects_non_numeric_ttl_constraints() {
    let policy = serde_json::json!({
        "permission": [{
            "constraint": {
                "leftOperand": "credential-ttl",
                "operator": "lteq",
                "rightOperand": "five minutes"
            }
        }]
    });

    let err = ReadRestriction::from_odrl_policies([&policy]).unwrap_err();

    assert!(
        err.to_string()
            .contains("ODRL max credential TTL must be an unsigned integer")
    );
}

#[test]
fn read_restriction_rejects_non_object_row_predicates() {
    let policy = serde_json::json!({
        "read-restriction": {
            "row-predicate": "severity >= 3"
        }
    });

    let err = ReadRestriction::from_odrl_policies([&policy]).unwrap_err();

    assert!(
        err.to_string()
            .contains("ODRL row predicate must be an Iceberg expression object")
    );
}

#[test]
fn read_restriction_narrows_projection_stats_and_filters() {
    let row_predicate = serde_json::json!({
        "type": "equal",
        "term": "region",
        "value": "west"
    });
    let restriction = ReadRestriction {
        allowed_columns: Some(vec!["event_id".to_string(), "severity".to_string()]),
        row_predicate: Some(row_predicate.clone()),
        ..ReadRestriction::unrestricted()
    };

    assert_eq!(
        restriction.effective_projection(&[]).unwrap(),
        vec!["event_id".to_string(), "severity".to_string()]
    );
    assert_eq!(
        restriction
            .effective_projection(&["event_id".to_string(), "payload".to_string()])
            .unwrap(),
        vec!["event_id".to_string()]
    );
    assert!(
        restriction
            .effective_projection(&["payload".to_string()])
            .is_err()
    );
    assert_eq!(
        restriction.effective_stats_fields(&["payload".to_string(), "severity".to_string()]),
        vec!["severity".to_string()]
    );
    assert_eq!(restriction.mandatory_filters(), vec![row_predicate]);
}

#[test]
fn table_capabilities_require_matching_allowed_receipts() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:reader".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::TablePlanScan,
        table: Some(table.clone()),
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };

    let capability = TableScanCapability::from_receipt(receipt.clone(), table.clone())
        .expect("matching receipt should mint capability");
    assert_eq!(capability.table(), &table);
    assert_eq!(capability.receipt(), &receipt);

    let other_table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("other").unwrap(),
    );
    assert!(TableScanCapability::from_receipt(receipt.clone(), other_table).is_err());

    let mut wrong_action_receipt = receipt;
    wrong_action_receipt.action = CatalogAction::TableLoad;
    assert!(TableScanCapability::from_receipt(wrong_action_receipt, table.clone()).is_err());

    let load_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:reader".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::TableLoad,
        table: Some(capability.table().clone()),
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    let load_capability =
        TableLoadCapability::from_receipt(load_receipt, capability.table().clone())
            .expect("matching load receipt should mint capability");
    assert_eq!(load_capability.table(), capability.table());

    let commit_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:writer".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::TableCommit,
        table: Some(capability.table().clone()),
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    let commit_capability =
        TableCommitCapability::from_receipt(commit_receipt, capability.table().clone())
            .expect("matching commit receipt should mint capability");
    assert_eq!(commit_capability.table(), capability.table());

    let drop_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:writer".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::TableDrop,
        table: Some(capability.table().clone()),
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    let drop_capability =
        TableDropCapability::from_receipt(drop_receipt, capability.table().clone())
            .expect("matching drop receipt should mint capability");
    assert_eq!(drop_capability.table(), capability.table());

    let create_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:writer".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::TableCreate,
        table: Some(capability.table().clone()),
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    let create_capability =
        TableCreateCapability::from_receipt(create_receipt, capability.table().clone())
            .expect("matching create receipt should mint capability");
    assert_eq!(create_capability.table(), capability.table());

    let credentials_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:reader".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::CredentialsVend,
        table: Some(capability.table().clone()),
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    let credentials_capability =
        CredentialsVendCapability::from_receipt(credentials_receipt, capability.table().clone())
            .expect("matching credential receipt should mint capability");
    assert_eq!(credentials_capability.table(), capability.table());

    let graph_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:querygraph".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::GraphRead,
        table: None,
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    let graph_capability = GraphReadCapability::from_receipt(graph_receipt.clone())
        .expect("matching graph-read receipt should mint capability");
    assert_eq!(graph_capability.receipt(), &graph_receipt);

    let mut table_scoped_graph_receipt = graph_receipt;
    table_scoped_graph_receipt.table = Some(capability.table().clone());
    assert!(GraphReadCapability::from_receipt(table_scoped_graph_receipt).is_err());

    let config_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:catalog".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::CatalogConfig,
        table: None,
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    assert!(CatalogConfigCapability::from_receipt(config_receipt).is_ok());

    let namespace_create_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:catalog".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::NamespaceCreate,
        table: None,
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    assert!(NamespaceCreateCapability::from_receipt(namespace_create_receipt).is_ok());

    let namespace_list_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:catalog".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::NamespaceList,
        table: None,
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    assert!(NamespaceListCapability::from_receipt(namespace_list_receipt).is_ok());

    let namespace_load_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:catalog".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::NamespaceLoad,
        table: None,
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    assert!(NamespaceLoadCapability::from_receipt(namespace_load_receipt).is_ok());

    let namespace_drop_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:catalog".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::NamespaceDrop,
        table: None,
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    assert!(NamespaceDropCapability::from_receipt(namespace_drop_receipt).is_ok());

    let server_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:operator".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::ServerManage,
        table: None,
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    assert!(ServerManageCapability::from_receipt(server_receipt).is_ok());

    let project_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:operator".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::ProjectManage,
        table: None,
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    assert!(ProjectManageCapability::from_receipt(project_receipt).is_ok());

    let warehouse_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:operator".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::WarehouseManage,
        table: None,
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    assert!(WarehouseManageCapability::from_receipt(warehouse_receipt).is_ok());

    let storage_profile_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:operator".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::StorageProfileManage,
        table: None,
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    assert!(StorageProfileManageCapability::from_receipt(storage_profile_receipt).is_ok());

    let view_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:operator".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::ViewManage,
        table: None,
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    assert!(ViewManageCapability::from_receipt(view_receipt).is_ok());

    let view_load_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:reader".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::ViewLoad,
        table: None,
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    assert!(ViewLoadCapability::from_receipt(view_load_receipt).is_ok());

    let view_drop_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:operator".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::ViewDrop,
        table: None,
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    assert!(ViewDropCapability::from_receipt(view_drop_receipt).is_ok());

    let policy_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:operator".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::PolicyManage,
        table: None,
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    assert!(PolicyManageCapability::from_receipt(policy_receipt).is_ok());

    let restore_receipt = AuthorizationReceipt {
        principal: Principal {
            subject: "agent:operator".to_string(),
            kind: PrincipalKind::Agent,
        },
        action: CatalogAction::TableRestore,
        table: Some(table.clone()),
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: Utc::now(),
    };
    assert!(TableRestoreCapability::from_receipt(restore_receipt, table).is_ok());
}

#[test]
fn authorization_receipt_hashes_enforced_read_restriction_policies() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let policy_hash = content_hash_json(&json!({
        "uid": "policy:agent-read",
        "lakecat:read-restriction": {"allowed-columns": ["event_id"]}
    }))
    .unwrap();
    let receipt = AuthorizationReceipt {
        principal: Principal::anonymous(),
        action: CatalogAction::TablePlanScan,
        table: Some(table),
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: json!({
            "read-restriction": {
                "allowed-columns": ["event_id"],
                "policy-hashes": [policy_hash.clone()]
            }
        }),
        checked_at: Utc::now(),
    }
    .with_read_restriction_policy_hash()
    .unwrap();

    let receipt_policy_hash = receipt.policy_hash.as_deref().expect("receipt policy hash");
    assert_ne!(receipt_policy_hash, policy_hash);
    assert_eq!(
        receipt.context["read-restriction"]["policy-hashes"][0],
        policy_hash
    );
}

#[tokio::test]
async fn allow_all_governance_receipt_names_local_engine() {
    let engine = AllowAllGovernanceEngine::new();
    let receipt = engine
        .authorize(AuthorizationRequest {
            principal: Principal::anonymous(),
            action: CatalogAction::CatalogConfig,
            table: None,
            context: json!({"warehouse": "local"}),
        })
        .await
        .expect("allow-all governance should authorize");

    assert!(receipt.allowed);
    assert_eq!(receipt.engine, ALLOW_ALL_LOCAL_ENGINE);
    assert!(!receipt.engine.contains("placeholder"));
    assert_eq!(receipt.context["warehouse"], "local");
}
