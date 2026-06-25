use crate::*;

pub(crate) fn join_u64s(values: &[u64]) -> String {
    values
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) async fn drain_lineage_outbox(
    catalog: &str,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<LineageDrainResponse> {
    drain_lineage_outbox_with_identity(catalog, principal, RequestIdentityMode::Principal).await
}

pub(crate) async fn drain_lineage_outbox_with_identity(
    catalog: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<LineageDrainResponse> {
    post_json_with_identity::<_, LineageDrainResponse>(
        catalog,
        "/management/v1/lineage/drain",
        principal,
        identity_mode,
        "lineage drain",
        &json!({}),
    )
    .await
}
