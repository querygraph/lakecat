use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lakecat_core::{LakeCatResult, Principal, TableIdent, content_hash_json};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[async_trait]
pub trait LineageSink: Send + Sync + 'static {
    async fn emit(&self, event: LineageEvent) -> LakeCatResult<LineageReceipt>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineageEvent {
    pub event_type: LineageEventType,
    pub principal: Principal,
    pub table: Option<TableIdent>,
    pub payload: Value,
    pub emitted_at: DateTime<Utc>,
}

impl LineageEvent {
    pub fn new(
        event_type: LineageEventType,
        principal: Principal,
        table: Option<TableIdent>,
        payload: Value,
    ) -> Self {
        Self {
            event_type,
            principal,
            table,
            payload,
            emitted_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LineageEventType {
    NamespaceCreated,
    TableCreated,
    TableLoaded,
    TableScanPlanned,
    TableCommitted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineageReceipt {
    pub event_hash: String,
    pub sink: String,
}

#[derive(Debug, Default)]
pub struct HashOnlyLineageSink;

impl HashOnlyLineageSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl LineageSink for HashOnlyLineageSink {
    async fn emit(&self, event: LineageEvent) -> LakeCatResult<LineageReceipt> {
        let event_hash = content_hash_json(&serde_json::to_value(&event).map_err(|err| {
            lakecat_core::LakeCatError::Internal(format!("failed to encode lineage event: {err}"))
        })?)?;
        Ok(LineageReceipt {
            event_hash,
            sink: "lakecat-hash-only-openlineage-placeholder".to_string(),
        })
    }
}
