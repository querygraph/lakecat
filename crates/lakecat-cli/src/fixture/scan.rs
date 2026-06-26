use crate::*;

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_governed_scan(
    catalog: &str,
    namespace_path: &str,
    table: &str,
    table_location: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let plan = post_json_with_identity::<_, PlanTableScanResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}/plan"),
        principal,
        identity_mode,
        "qglake governed scan plan",
        &PlanTableScanRequest {
            select: vec![
                "event_id".to_string(),
                "occurred_at".to_string(),
                "severity".to_string(),
                "raw_payload".to_string(),
            ],
            stats_fields: vec![
                "event_id".to_string(),
                "occurred_at".to_string(),
                "severity".to_string(),
                "raw_payload".to_string(),
            ],
            case_sensitive: Some(true),
            ..empty_scan_request()
        },
    )
    .await?;
    verify_qglake_scan_plan(&plan)?;
    verify_qglake_fetch_scan_tasks(
        catalog,
        namespace_path,
        table,
        table_location,
        principal,
        identity_mode,
        &plan,
    )
    .await
}

#[cfg(feature = "qglake-fixture")]
pub(crate) fn empty_scan_request() -> PlanTableScanRequest {
    PlanTableScanRequest {
        projection: Vec::new(),
        select: Vec::new(),
        filters: Vec::new(),
        filter: None,
        limit: None,
        snapshot_id: None,
        case_sensitive: None,
        use_snapshot_schema: None,
        start_snapshot_id: None,
        end_snapshot_id: None,
        stats_fields: Vec::new(),
    }
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_scan_plan(
    plan: &PlanTableScanResponse,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_sail_planner("scan plan", &plan.planned_by)?;
    if plan.plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed scan plan did not expose an Iceberg REST plan-task token".to_string(),
        ));
    }
    if !plan
        .lakecat_plan_tasks
        .iter()
        .any(|task| task["task-type"] == json!("manifest-list"))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed scan plan did not include a manifest-list task".to_string(),
        ));
    }
    let extension = plan
        .residual_filter
        .as_ref()
        .and_then(|filter| filter.get("lakecat:scan-request"))
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "qglake governed scan plan did not include lakecat:scan-request".to_string(),
            )
        })?;
    if extension.get("effective-projection")
        != Some(&json!(["event_id", "occurred_at", "severity"]))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed scan effective projection was not narrowed as expected: {}",
            extension
                .get("effective-projection")
                .cloned()
                .unwrap_or(Value::Null)
        )));
    }
    if extension.get("requested-stats-fields")
        != Some(&json!([
            "event_id",
            "occurred_at",
            "severity",
            "raw_payload"
        ]))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed scan requested stats fields were not preserved as expected: {}",
            extension
                .get("requested-stats-fields")
                .cloned()
                .unwrap_or(Value::Null)
        )));
    }
    for field in ["effective-stats-fields", "stats-fields"] {
        if extension.get(field) != Some(&json!(["event_id", "occurred_at", "severity"])) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake governed scan {field} were not narrowed as expected: {}",
                extension.get(field).cloned().unwrap_or(Value::Null)
            )));
        }
    }
    verify_qglake_plan_or_fetch_read_restriction(
        &extension["read-restriction"],
        plan.table.name.as_str(),
        "qglake governed scan",
    )?;
    Ok(())
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_sail_planner(
    label: &str,
    planned_by: &str,
) -> lakecat_core::LakeCatResult<()> {
    if planned_by != "sail-rest-models" {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed {label} was not planned by Sail REST models: {planned_by}"
        )));
    }
    Ok(())
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn qglake_policy_hash(table: &str) -> lakecat_core::LakeCatResult<String> {
    content_hash_json(&qglake_odrl_policy(table))
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_fetch_scan_tasks(
    catalog: &str,
    namespace_path: &str,
    table: &str,
    table_location: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
    plan: &PlanTableScanResponse,
) -> lakecat_core::LakeCatResult<()> {
    let plan_task = plan.plan_tasks.first().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed scan plan did not produce a plan-task token for fetch verification"
                .to_string(),
        )
    })?;
    let fetched = post_json_with_identity::<_, FetchScanTasksResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}/tasks"),
        principal,
        identity_mode,
        "qglake governed scan task fetch",
        &FetchScanTasksRequest {
            plan_task: plan_task.clone(),
        },
    )
    .await?;
    verify_qglake_scan_tasks(&fetched, table_location)?;
    if fetched.plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks did not produce child plan-task tokens for manifest fetch verification"
                .to_string(),
        ));
    }
    for (index, child_plan_task) in fetched.plan_tasks.iter().enumerate() {
        let manifest_fetched = post_json_with_identity::<_, FetchScanTasksResponse>(
            catalog,
            &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}/tasks"),
            principal,
            identity_mode,
            "qglake governed manifest scan task fetch",
            &FetchScanTasksRequest {
                plan_task: child_plan_task.clone(),
            },
        )
        .await?;
        let child_descriptor = fetched.lakecat_plan_tasks.get(index);
        if child_descriptor.and_then(|task| task["content"].as_str()) == Some("deletes") {
            verify_qglake_delete_manifest_scan_tasks(&manifest_fetched, table_location)?;
        } else {
            verify_qglake_leaf_scan_tasks(&manifest_fetched, table_location)?;
        }
    }
    Ok(())
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_scan_tasks(
    fetched: &FetchScanTasksResponse,
    table_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_scan_task_common(fetched, table_location)?;
    if fetched.plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks did not expose a child Iceberg REST plan-task token"
                .to_string(),
        ));
    }
    if !fetched
        .lakecat_plan_tasks
        .iter()
        .any(|task| task["task-type"] == json!("manifest"))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks did not include a manifest child task".to_string(),
        ));
    }
    if fetched.delete_files.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks did not expose Iceberg delete-file refs".to_string(),
        ));
    }
    if !fetched.file_scan_tasks.iter().any(|task| {
        task.get("delete-file-references")
            .and_then(Value::as_array)
            .is_some_and(|refs| !refs.is_empty())
    }) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks did not attach delete-file references to data tasks"
                .to_string(),
        ));
    }
    Ok(())
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_leaf_scan_tasks(
    fetched: &FetchScanTasksResponse,
    table_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_scan_task_common(fetched, table_location)?;
    if !fetched.plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed manifest fetchScanTasks unexpectedly exposed {} child plan-task token(s)",
            fetched.plan_tasks.len()
        )));
    }
    if !fetched.lakecat_plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed manifest fetchScanTasks unexpectedly included {} LakeCat child task(s)",
            fetched.lakecat_plan_tasks.len()
        )));
    }
    Ok(())
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_delete_manifest_scan_tasks(
    fetched: &FetchScanTasksResponse,
    table_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_sail_planner("delete manifest fetchScanTasks", &fetched.planned_by)?;
    if !fetched.plan_tasks.is_empty() || !fetched.lakecat_plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed delete manifest fetchScanTasks unexpectedly exposed child tasks"
                .to_string(),
        ));
    }
    if !fetched.file_scan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed delete manifest fetchScanTasks unexpectedly exposed data scan tasks"
                .to_string(),
        ));
    }
    if fetched.delete_files.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed delete manifest fetchScanTasks produced no delete files".to_string(),
        ));
    }
    let table_prefix = format!("{}/", table_location.trim_end_matches('/'));
    for delete_file_path in fetched
        .delete_files
        .iter()
        .filter_map(|file| file.get("file-path").and_then(Value::as_str))
    {
        if !delete_file_path.starts_with(&table_prefix) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake governed delete file escaped table location {table_location}: {delete_file_path}"
            )));
        }
    }
    verify_qglake_fetch_restriction(fetched)?;
    Ok(())
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_scan_task_common(
    fetched: &FetchScanTasksResponse,
    table_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_sail_planner("fetchScanTasks", &fetched.planned_by)?;
    if fetched.file_scan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks produced no file scan tasks".to_string(),
        ));
    }
    let data_file_paths = fetched
        .file_scan_tasks
        .iter()
        .filter_map(|task| task.pointer("/data-file/file-path").and_then(Value::as_str))
        .collect::<Vec<_>>();
    if data_file_paths.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks produced no data-file file paths".to_string(),
        ));
    }
    let table_prefix = format!("{}/", table_location.trim_end_matches('/'));
    for data_file_path in data_file_paths {
        if !data_file_path.starts_with(&table_prefix) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake governed fetchScanTasks data file escaped table location {table_location}: {data_file_path}"
            )));
        }
    }
    verify_qglake_fetch_restriction(fetched)
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_fetch_restriction(
    fetched: &FetchScanTasksResponse,
) -> lakecat_core::LakeCatResult<()> {
    let extension = fetched
        .residual_filter
        .as_ref()
        .and_then(|filter| filter.get("lakecat:fetch-scan-tasks"))
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "qglake governed fetchScanTasks did not include lakecat:fetch-scan-tasks"
                    .to_string(),
            )
        })?;
    verify_qglake_plan_or_fetch_read_restriction(
        &extension["read-restriction"],
        fetched.table.name.as_str(),
        "qglake governed fetchScanTasks",
    )?;
    if extension["required-projection"] != json!(["event_id", "occurred_at", "severity"]) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed fetchScanTasks required projection did not prove re-applied narrowing: {}",
            extension["required-projection"].clone()
        )));
    }
    if extension["effective-projection"] != json!(["event_id", "occurred_at", "severity"]) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed fetchScanTasks effective projection did not prove re-applied narrowing: {}",
            extension["effective-projection"].clone()
        )));
    }
    if extension["required-filters"]
        .as_array()
        .and_then(|filters| filters.first())
        != Some(&json!({
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        }))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed fetchScanTasks required filters did not prove re-applied row predicate: {}",
            extension["required-filters"].clone()
        )));
    }
    Ok(())
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_plan_or_fetch_read_restriction(
    restriction: &Value,
    table: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if restriction["allowed-columns"] != json!(["event_id", "occurred_at", "severity"]) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} allowed columns were not narrowed as expected: {}",
            restriction["allowed-columns"].clone()
        )));
    }
    if restriction["row-predicate"]
        != json!({
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        })
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} row predicate was not enforced as expected: {}",
            restriction["row-predicate"].clone()
        )));
    }
    if restriction["purpose"] != json!("qglake-agent-demo") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} purpose was not preserved as expected: {}",
            restriction["purpose"].clone()
        )));
    }
    if restriction["max-credential-ttl-seconds"] != json!(300) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} max credential TTL was not preserved as expected: {}",
            restriction["max-credential-ttl-seconds"].clone()
        )));
    }
    let expected_policy_hash = qglake_policy_hash(table)?;
    let policy_hashes = restriction["policy-hashes"].as_array().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} read restriction did not include policy hashes"
        ))
    })?;
    if !policy_hashes
        .iter()
        .any(|hash| hash.as_str() == Some(expected_policy_hash.as_str()))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} did not bind to expected ODRL policy hash {expected_policy_hash}: {}",
            restriction["policy-hashes"].clone()
        )));
    }
    Ok(())
}
