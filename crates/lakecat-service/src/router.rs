use axum::Router;
use axum::routing::{get, post};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};

use crate::*;

pub fn app(state: LakeCatState) -> Router {
    Router::new()
        .route("/catalog/v1/config", get(get_config))
        .route(
            "/catalog/v1/{warehouse}/config",
            get(get_config_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces",
            get(list_namespaces_for_warehouse).post(create_namespace_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}",
            get(load_namespace_for_warehouse).delete(drop_namespace_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/tables",
            post(create_table_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}",
            // Iceberg REST `updateTable` is a bare POST on the table path. The
            // `/commit` route below is kept as a LakeCat alias.
            get(load_table_for_warehouse)
                .post(commit_table_for_warehouse)
                .delete(delete_table_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/commit",
            post(commit_table_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/plan",
            post(plan_table_scan_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
            post(fetch_scan_tasks_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/tasks",
            post(fetch_scan_tasks_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/credentials",
            get(load_credentials_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/views",
            get(catalog_list_views),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/views/{view}",
            get(catalog_load_view)
                .post(catalog_upsert_view)
                .put(catalog_upsert_view)
                .delete(catalog_drop_view),
        )
        .route(
            "/catalog/v1/namespaces",
            get(list_namespaces).post(create_namespace),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}",
            get(load_namespace).delete(drop_namespace),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables",
            post(create_table),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}",
            // Iceberg REST `updateTable` is a bare POST on the table path. The
            // `/commit` route below is kept as a LakeCat alias.
            get(load_table)
                .post(commit_table)
                .delete(delete_table),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}/commit",
            post(commit_table),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}/plan",
            post(plan_table_scan),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
            post(fetch_scan_tasks),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}/tasks",
            post(fetch_scan_tasks),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}/credentials",
            get(load_credentials),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace}/tables/{table}/restore",
            post(restore_table),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace}/tables/{table}/commits",
            get(list_table_commits),
        )
        .route("/management/v1/projects", get(list_projects))
        .route(
            "/management/v1/projects/{project}",
            post(upsert_project).put(upsert_project),
        )
        .route(
            "/management/v1/projects/{project}/warehouses",
            get(list_project_warehouses),
        )
        .route(
            "/management/v1/projects/{project}/warehouses/{warehouse}",
            post(upsert_project_warehouse).put(upsert_project_warehouse),
        )
        .route("/management/v1/warehouses", get(list_warehouses))
        .route(
            "/management/v1/warehouses/{warehouse}",
            post(upsert_warehouse).put(upsert_warehouse),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/storage-profiles",
            get(list_storage_profiles),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/storage-profiles/{profile}",
            post(upsert_storage_profile).put(upsert_storage_profile),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace}/views",
            get(list_views),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace}/views/{view}",
            post(upsert_view).put(upsert_view).delete(drop_view),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace}/views/{view}/version-receipts",
            get(list_view_version_receipts),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace}/view-version-receipt-chains",
            get(list_view_version_receipt_chains),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/policies",
            get(list_policy_bindings),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/policies/{policy}",
            post(upsert_policy_binding).put(upsert_policy_binding),
        )
        .route("/management/v1/lineage/drain", post(drain_lineage_outbox))
        .route("/management/v1/servers", get(list_servers))
        .route(
            "/management/v1/servers/{server}",
            post(upsert_server).put(upsert_server),
        )
        .route("/querygraph/v1/bootstrap", get(querygraph_bootstrap))
        .with_state(state)
}
