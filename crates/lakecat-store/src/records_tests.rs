use chrono::Utc;
use lakecat_core::{Namespace, Principal, TableIdent, TableName, WarehouseName};

use super::*;

fn commit_record(snapshot_id: i64) -> TableCommitRecord {
    TableCommitRecord {
        table: TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        ),
        previous_metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
        new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
        sequence_number: 1,
        principal: Principal::anonymous(),
        format_version: Some(2),
        snapshot_id: Some(snapshot_id),
        policy_hash: None,
        request_hash: format!("sha256:{}", "0".repeat(64)),
        response_hash: format!("sha256:{}", "1".repeat(64)),
        idempotency_key_sha256: None,
        committed_at: Utc::now(),
    }
}

#[test]
fn legacy_iceberg_no_snapshot_commit_record_decodes_to_zero_evidence() {
    let mut value = serde_json::to_value(commit_record(0)).unwrap();
    value["snapshot_id"] = serde_json::json!(-1);

    let record: TableCommitRecord = serde_json::from_value(value).unwrap();

    assert_eq!(record.snapshot_id, Some(0));
    record.validate_for_table(&record.table).unwrap();
}

#[test]
fn other_negative_commit_snapshot_ids_remain_invalid() {
    let mut value = serde_json::to_value(commit_record(0)).unwrap();
    value["snapshot_id"] = serde_json::json!(-2);

    let record: TableCommitRecord = serde_json::from_value(value).unwrap();

    assert_eq!(record.snapshot_id, Some(-2));
    let err = record.validate_for_table(&record.table).unwrap_err();
    assert!(
        err.to_string()
            .contains("table commit record snapshot id must be non-negative")
    );
}
