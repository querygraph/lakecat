use super::common::*;
use crate::*;

#[test]
fn qglake_credentials_verifier_requires_empty_raw_credentials() {
    verify_qglake_credentials_response(&LoadCredentialsResponse {
        storage_credentials: Vec::new(),
    })
    .expect("QGLake restricted table should accept empty raw credentials");

    let err = verify_qglake_credentials_response(&LoadCredentialsResponse {
        storage_credentials: vec![lakecat_api::StorageCredential {
            prefix: "file:///tmp/lakecat-qglake/events".to_string(),
            config: vec![lakecat_api::ConfigEntry::new(
                "lakecat.credential-mode",
                "local-file-no-secret",
            )],
        }],
    })
    .expect_err("QGLake restricted table should reject raw credentials");
    assert!(
        err.to_string()
            .contains("qglake restricted table unexpectedly returned")
    );
}

#[test]
fn qglake_trusted_human_credentials_verifier_requires_standard_local_credentials() {
    verify_qglake_trusted_human_credentials_response(
        &LoadCredentialsResponse {
            storage_credentials: vec![lakecat_api::StorageCredential {
                prefix: QGLAKE_TEST_LOCATION.to_string(),
                config: vec![lakecat_api::ConfigEntry::new(
                    "lakecat.credential-mode",
                    "local-file-no-secret",
                )],
            }],
        },
        QGLAKE_TEST_LOCATION,
    )
    .expect("trusted human path should accept standard non-secret credentials");

    let err = verify_qglake_trusted_human_credentials_response(
        &LoadCredentialsResponse {
            storage_credentials: Vec::new(),
        },
        QGLAKE_TEST_LOCATION,
    )
    .expect_err("trusted human path should require a standard credential set");
    assert!(
        err.to_string()
            .contains("returned no standard credential set")
    );

    let err = verify_qglake_trusted_human_credentials_response(
        &LoadCredentialsResponse {
            storage_credentials: vec![lakecat_api::StorageCredential {
                prefix: QGLAKE_TEST_LOCATION.to_string(),
                config: vec![lakecat_api::ConfigEntry::new("aws.session-token", "token")],
            }],
        },
        QGLAKE_TEST_LOCATION,
    )
    .expect_err("trusted human local credentials should not expose secrets");
    assert!(err.to_string().contains("secret material"));
}

#[test]
fn qglake_credential_replay_line_summarizes_verified_evidence() {
    let line = qglake_credential_replay_line(
        &LineageDrainResponse {
            delivered: 2,
            event_types: vec![
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
            ],
            graph_events: 0,
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
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
            ],
        },
        Some("did:example:agent"),
    )
    .expect("credential replay line should be present");

    assert_eq!(
        line,
        "credential replay restricted=blocked:sail-planned-read-required restricted_count=0 restricted_ttl=300 restricted_profile=events-local:file:local-file-no-secret:location_prefix_hash=sha256:2222222222222222222222222222222222222222222222222222222222222222:secret_ref=none:graph_events=2 human=allowed:trusted-human-audited-raw human_count=1 human_ttl=300 human_profile=events-local:file:local-file-no-secret:location_prefix_hash=sha256:2222222222222222222222222222222222222222222222222222222222222222:secret_ref=none:graph_events=2"
    );
}

#[test]
fn qglake_credential_replay_line_summarizes_secret_ref_hashes() {
    let secret_ref_hash = qglake_fixture_hash("qglake-production-secret-ref");
    let mut restricted = qglake_restricted_credential_summary();
    let mut human = qglake_human_credential_summary();
    for event in [&mut restricted, &mut human] {
        event.storage_profile_provider = Some("s3".to_string());
        event.storage_profile_issuance_mode = Some("short-lived-secret-ref".to_string());
        event.storage_profile_secret_ref_present = Some(true);
        event.storage_profile_secret_ref_provider = Some("typesec".to_string());
        event.storage_profile_secret_ref_hash = Some(secret_ref_hash.clone());
    }

    let line = qglake_credential_replay_line(
        &LineageDrainResponse {
            delivered: 2,
            event_types: vec![
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
            ],
            graph_events: 0,
            lineage_events: 2,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![restricted, human],
        },
        Some("did:example:agent"),
    )
    .expect("credential replay line should summarize secret-ref evidence");

    assert!(line.contains("restricted_profile=events-local:s3:short-lived-secret-ref"));
    assert!(line.contains("human_profile=events-local:s3:short-lived-secret-ref"));
    assert!(line.contains(&format!(
        "secret_ref=typesec:secret_ref_hash={secret_ref_hash}"
    )));
}

#[test]
fn qglake_credential_replay_line_rejects_short_secret_ref_hashes() {
    let mut restricted = qglake_restricted_credential_summary();
    let mut human = qglake_human_credential_summary();
    for event in [&mut restricted, &mut human] {
        event.storage_profile_provider = Some("s3".to_string());
        event.storage_profile_issuance_mode = Some("short-lived-secret-ref".to_string());
        event.storage_profile_secret_ref_present = Some(true);
        event.storage_profile_secret_ref_provider = Some("typesec".to_string());
        event.storage_profile_secret_ref_hash = Some("sha256:short-secret-ref".to_string());
    }

    let line = qglake_credential_replay_line(
        &LineageDrainResponse {
            delivered: 2,
            event_types: vec![
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
            ],
            graph_events: 0,
            lineage_events: 2,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![restricted, human],
        },
        Some("did:example:agent"),
    );

    assert!(
        line.is_none(),
        "credential replay line must not summarize short secret-ref hash evidence"
    );
}
