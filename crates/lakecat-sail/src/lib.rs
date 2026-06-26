// Re-export the Sail seam types from lakecat-core so downstream crates
// that only need the trait and request/response types don't have to
// depend on lakecat-sail (which has local Sail path deps).
pub use lakecat_core::sail::{
    CommitPlan, CommitPreparationRequest, DeferredSailCatalogEngine, FetchScanTasksPlan,
    FetchScanTasksRequest, IcebergFormatSupport, SailCatalogEngine, SailFieldSummary,
    SailFilterSummary, SailMetadataSummary, SailScanTask, ScanPlan, ScanPlanningRequest,
    validate_lakecat_metadata_format,
};

#[cfg(feature = "catalog-provider")]
pub mod catalog_provider;

#[cfg(feature = "sail-local")]
pub mod sail_integration;
