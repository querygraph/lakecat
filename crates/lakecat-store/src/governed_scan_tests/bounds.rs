use lakecat_core::LakeCatError;
use lakecat_core::governed_scan::{
    MAX_GOVERNED_SCAN_PROJECTION_BYTES, MAX_GOVERNED_SCAN_PROJECTION_FIELDS,
    MAX_GOVERNED_SCAN_TEXT_BYTES,
};

use super::sample_grant;

#[test]
fn grant_policy_engine_text_limit_is_inclusive() {
    let mut exact = sample_grant();
    exact.policy_engine = "e".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES);
    exact.validate().unwrap();

    for invalid in [
        "e".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES + 1),
        " leading".to_string(),
        "control\nengine".to_string(),
    ] {
        let mut malformed = sample_grant();
        malformed.policy_engine = invalid;
        assert_invalid_argument(malformed.validate().unwrap_err());
    }
}

#[test]
fn requested_projection_count_and_aggregate_limits_are_inclusive() {
    let mut exact_count = sample_grant();
    exact_count.requested_projection = (0..MAX_GOVERNED_SCAN_PROJECTION_FIELDS)
        .map(|index| format!("field_{index:03}"))
        .collect();
    exact_count.validate().unwrap();
    exact_count
        .requested_projection
        .push("field_over_limit".to_string());
    assert_invalid_argument(exact_count.validate().unwrap_err());

    let mut exact_bytes = sample_grant();
    exact_bytes.requested_projection = bounded_names(32, MAX_GOVERNED_SCAN_PROJECTION_BYTES);
    exact_bytes.validate().unwrap();
    exact_bytes.requested_projection[0].push('x');
    assert_invalid_argument(exact_bytes.validate().unwrap_err());
}

#[test]
fn requested_projection_may_be_empty_but_must_otherwise_be_canonical() {
    let mut empty = sample_grant();
    empty.requested_projection.clear();
    empty.validate().unwrap();

    for projection in [
        vec!["f".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES + 1)],
        vec![" leading".to_string()],
        vec!["control\nfield".to_string()],
        vec!["duplicate".to_string(), "duplicate".to_string()],
    ] {
        let mut malformed = sample_grant();
        malformed.requested_projection = projection;
        assert_invalid_argument(malformed.validate().unwrap_err());
    }
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
            format!("{}{}", "f".repeat(prefix_bytes), suffix)
        })
        .collect()
}

fn assert_invalid_argument(error: LakeCatError) {
    assert!(matches!(error, LakeCatError::InvalidArgument(_)));
}
