use crate::*;

#[test]
fn parses_config_command_defaults() {
    let command = Command::parse(["config".to_string()]).unwrap();
    match command {
        Command::Config { catalog, principal } => {
            assert_eq!(catalog, "http://127.0.0.1:8181");
            assert_eq!(principal, None);
        }
        _ => panic!("expected config command"),
    }
}

#[test]
fn parses_bootstrap_export_command() {
    let command = Command::parse([
        "bootstrap-export".to_string(),
        "--catalog".to_string(),
        "http://localhost:9000".to_string(),
        "--output".to_string(),
        "bundle.json".to_string(),
        "--principal".to_string(),
        "alice".to_string(),
    ])
    .unwrap();
    match command {
        Command::BootstrapExport {
            catalog,
            output,
            principal,
        } => {
            assert_eq!(catalog, "http://localhost:9000");
            assert_eq!(output, PathBuf::from("bundle.json"));
            assert_eq!(principal.as_deref(), Some("alice"));
        }
        _ => panic!("expected bootstrap-export command"),
    }
}

#[test]
fn parses_lineage_drain_command() {
    let command = Command::parse([
        "lineage-drain".to_string(),
        "--catalog".to_string(),
        "http://localhost:9000".to_string(),
        "--principal".to_string(),
        "did:example:agent".to_string(),
    ])
    .unwrap();
    match command {
        Command::LineageDrain { catalog, principal } => {
            assert_eq!(catalog, "http://localhost:9000");
            assert_eq!(principal.as_deref(), Some("did:example:agent"));
        }
        _ => panic!("expected lineage-drain command"),
    }
}

#[test]
fn parses_qglake_verify_replay_command() {
    let command = Command::parse([
        "qglake-verify-replay".to_string(),
        "--bundle".to_string(),
        "target/qglake/lakecat-bootstrap.json".to_string(),
        "--drain".to_string(),
        "target/qglake/lineage-drain.json".to_string(),
        "--principal".to_string(),
        "did:example:agent".to_string(),
        "--json".to_string(),
    ])
    .unwrap();
    match command {
        Command::QglakeVerifyReplay {
            bundle,
            drain,
            principal,
            json,
        } => {
            assert_eq!(
                bundle,
                PathBuf::from("target/qglake/lakecat-bootstrap.json")
            );
            assert_eq!(drain, PathBuf::from("target/qglake/lineage-drain.json"));
            assert_eq!(principal.as_deref(), Some("did:example:agent"));
            assert!(json);
        }
        _ => panic!("expected qglake-verify-replay command"),
    }
}

#[test]
fn parses_qglake_verify_handoff_command() {
    let command = Command::parse([
        "qglake-verify-handoff".to_string(),
        "--summary".to_string(),
        "target/qglake-handoff/handoff-summary.json".to_string(),
        "--json".to_string(),
    ])
    .unwrap();
    match command {
        Command::QglakeVerifyHandoff { summary, json } => {
            assert_eq!(
                summary,
                PathBuf::from("target/qglake-handoff/handoff-summary.json")
            );
            assert!(json);
        }
        _ => panic!("expected qglake-verify-handoff command"),
    }
}

#[test]
fn parses_storage_profile_upsert_command() {
    let command = Command::parse([
        "storage-profile-upsert".to_string(),
        "--profile".to_string(),
        "local-events".to_string(),
        "--location-prefix".to_string(),
        "file:///tmp/events".to_string(),
        "--provider".to_string(),
        "file".to_string(),
        "--issuance-mode".to_string(),
        "local-file-no-secret".to_string(),
        "--public-config".to_string(),
        "lakecat.test=true".to_string(),
    ])
    .unwrap();
    match command {
        Command::StorageProfileUpsert {
            warehouse,
            profile,
            location_prefix,
            provider,
            issuance_mode,
            public_config,
            ..
        } => {
            assert_eq!(warehouse, "local");
            assert_eq!(profile, "local-events");
            assert_eq!(location_prefix, "file:///tmp/events");
            assert_eq!(provider, "file");
            assert_eq!(issuance_mode, "local-file-no-secret");
            assert_eq!(public_config["lakecat.test"], "true");
        }
        _ => panic!("expected storage-profile-upsert command"),
    }
}

#[test]
fn parses_policy_upsert_command() {
    let command = Command::parse([
        "policy-upsert".to_string(),
        "--policy".to_string(),
        "agent-read".to_string(),
        "--namespace".to_string(),
        "default.analytics".to_string(),
        "--table".to_string(),
        "events".to_string(),
        "--enforced".to_string(),
        "false".to_string(),
    ])
    .unwrap();
    match command {
        Command::PolicyUpsert {
            policy,
            namespace,
            table,
            enforced,
            ..
        } => {
            assert_eq!(policy, "agent-read");
            assert_eq!(
                namespace,
                Some(vec!["default".to_string(), "analytics".to_string()])
            );
            assert_eq!(table.as_deref(), Some("events"));
            assert!(!enforced);
        }
        _ => panic!("expected policy-upsert command"),
    }
}
