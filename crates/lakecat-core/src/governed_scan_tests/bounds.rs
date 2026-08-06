use serde_json::json;

use super::evidence;
use crate::governed_scan::{
    GovernedScanProof, GovernedScanProofEvidence, MAX_GOVERNED_SCAN_NAMESPACE_BYTES,
    MAX_GOVERNED_SCAN_NAMESPACE_COMPONENTS, MAX_GOVERNED_SCAN_PROJECTION_BYTES,
    MAX_GOVERNED_SCAN_PROJECTION_FIELDS, MAX_GOVERNED_SCAN_TEXT_BYTES, governed_evidence_digest,
};
use crate::{LakeCatError, Namespace, TableName, WarehouseName};

#[test]
fn subject_and_purpose_text_limits_are_inclusive() {
    for set_text in [
        |evidence: &mut GovernedScanProofEvidence, value| evidence.principal_subject = value,
        |evidence: &mut GovernedScanProofEvidence, value| evidence.purpose = value,
    ] {
        let mut exact = evidence();
        set_text(&mut exact, "x".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES));
        GovernedScanProof::issue(exact).unwrap();

        let mut over = evidence();
        set_text(&mut over, "x".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES + 1));
        assert_invalid_argument(GovernedScanProof::issue(over).unwrap_err());

        for invalid in [" leading", "trailing ", "control\ntext"] {
            let mut malformed = evidence();
            set_text(&mut malformed, invalid.to_string());
            assert_invalid_argument(GovernedScanProof::issue(malformed).unwrap_err());
        }
    }
}

#[test]
fn projection_count_and_aggregate_limits_are_inclusive() {
    let mut exact_count = evidence();
    exact_count.effective_projection = (0..MAX_GOVERNED_SCAN_PROJECTION_FIELDS)
        .map(|index| format!("field_{index:03}"))
        .collect();
    GovernedScanProof::issue(exact_count.clone()).unwrap();
    exact_count
        .effective_projection
        .push("field_over_limit".to_string());
    assert_invalid_argument(GovernedScanProof::issue(exact_count).unwrap_err());

    let mut exact_bytes = evidence();
    exact_bytes.effective_projection = bounded_names(32, MAX_GOVERNED_SCAN_PROJECTION_BYTES);
    GovernedScanProof::issue(exact_bytes.clone()).unwrap();
    exact_bytes.effective_projection[0].push('x');
    assert_invalid_argument(GovernedScanProof::issue(exact_bytes).unwrap_err());
}

#[test]
fn projection_field_limit_and_canonical_form_are_enforced() {
    let mut exact = evidence();
    exact.effective_projection = vec!["f".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES)];
    GovernedScanProof::issue(exact).unwrap();

    for projection in [
        vec!["f".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES + 1)],
        vec![" leading".to_string()],
        vec!["control\nfield".to_string()],
        vec!["duplicate".to_string(), "duplicate".to_string()],
    ] {
        let mut malformed = evidence();
        malformed.effective_projection = projection;
        assert_invalid_argument(GovernedScanProof::issue(malformed).unwrap_err());
    }
}

#[test]
fn namespace_count_and_aggregate_limits_are_inclusive() {
    let mut exact_count = evidence();
    exact_count.table.namespace = Namespace::new(
        (0..MAX_GOVERNED_SCAN_NAMESPACE_COMPONENTS)
            .map(|index| format!("part_{index:03}"))
            .collect(),
    )
    .unwrap();
    GovernedScanProof::issue(exact_count.clone()).unwrap();
    let mut over_count = exact_count.table.namespace.parts().to_vec();
    over_count.push("part_over_limit".to_string());
    exact_count.table.namespace = Namespace::new(over_count).unwrap();
    assert_invalid_argument(GovernedScanProof::issue(exact_count).unwrap_err());

    let mut exact_bytes = evidence();
    exact_bytes.table.namespace =
        Namespace::new(bounded_names(32, MAX_GOVERNED_SCAN_NAMESPACE_BYTES)).unwrap();
    GovernedScanProof::issue(exact_bytes.clone()).unwrap();
    let mut over_bytes = exact_bytes.table.namespace.parts().to_vec();
    over_bytes[0].push('x');
    exact_bytes.table.namespace = Namespace::new(over_bytes).unwrap();
    assert_invalid_argument(GovernedScanProof::issue(exact_bytes).unwrap_err());
}

#[test]
fn table_component_text_limits_are_inclusive() {
    let mut exact_warehouse = evidence();
    exact_warehouse.table.warehouse =
        WarehouseName::new("w".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES)).unwrap();
    GovernedScanProof::issue(exact_warehouse).unwrap();
    let mut over_warehouse = evidence();
    over_warehouse.table.warehouse =
        WarehouseName::new("w".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES + 1)).unwrap();
    assert_invalid_argument(GovernedScanProof::issue(over_warehouse).unwrap_err());

    let mut exact_table = evidence();
    exact_table.table.name = TableName::new("t".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES)).unwrap();
    GovernedScanProof::issue(exact_table).unwrap();
    let mut over_table = evidence();
    over_table.table.name = TableName::new("t".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES + 1)).unwrap();
    assert_invalid_argument(GovernedScanProof::issue(over_table).unwrap_err());
}

#[test]
fn deserialized_table_components_are_revalidated_without_echoing_input() {
    let base = GovernedScanProof::issue(evidence()).unwrap();
    for (field, value, secret) in [
        ("warehouse", json!("bad/warehouse"), "bad/warehouse"),
        ("namespace", json!(["bad namespace"]), "bad namespace"),
        ("name", json!(""), ""),
    ] {
        let mut encoded = serde_json::to_value(&base).unwrap();
        encoded["table"][field] = value;
        let malformed: GovernedScanProof = serde_json::from_value(encoded).unwrap();
        let error = malformed.validate_integrity().unwrap_err();
        assert_invalid_argument(error);
        if !secret.is_empty() {
            assert!(
                !malformed
                    .validate_structure()
                    .unwrap_err()
                    .to_string()
                    .contains(secret)
            );
        }
    }
}

#[test]
fn digest_shape_remains_compatible_with_the_v1_contract() {
    let evidence = evidence();
    let expected = governed_evidence_digest(
        "lakecat.governed-scan-proof.digest.v1",
        &json!({
            "version": "lakecat.governed-scan-proof.v1",
            "table": evidence.table,
            "tableVersion": evidence.table_version,
            "snapshotId": evidence.snapshot_id,
            "planTaskDigest": evidence.plan_task_digest,
            "principalSubject": evidence.principal_subject,
            "purpose": evidence.purpose,
            "effectiveProjection": evidence.effective_projection,
            "identityContextDigest": evidence.identity_context_digest,
            "authorizationReceiptDigest": evidence.authorization_receipt_digest,
            "policyDecisionDigest": evidence.policy_decision_digest,
        }),
    )
    .unwrap();
    assert_eq!(
        GovernedScanProof::issue(evidence).unwrap().grant_id,
        expected
    );
}

fn bounded_names(count: usize, total_bytes: usize) -> Vec<String> {
    let suffixes = (0..count)
        .map(|index| format!("_{index:03}"))
        .collect::<Vec<_>>();
    let suffix_bytes = suffixes.iter().map(String::len).sum::<usize>();
    assert!(total_bytes >= suffix_bytes + count);
    let mut remaining = total_bytes - suffix_bytes;
    suffixes
        .into_iter()
        .enumerate()
        .map(|(index, suffix)| {
            let slots = count - index;
            let prefix_bytes = remaining / slots;
            remaining -= prefix_bytes;
            assert!(prefix_bytes + suffix.len() <= MAX_GOVERNED_SCAN_TEXT_BYTES);
            format!("{}{}", "n".repeat(prefix_bytes), suffix)
        })
        .collect()
}

fn assert_invalid_argument(error: LakeCatError) {
    assert!(matches!(error, LakeCatError::InvalidArgument(_)));
}
