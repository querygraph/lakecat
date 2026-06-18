use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lakecat_core::{LakeCatError, LakeCatResult, Principal, TableIdent, content_hash_json};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[async_trait]
pub trait GovernanceEngine: Send + Sync + 'static {
    async fn authorize(&self, request: AuthorizationRequest)
    -> LakeCatResult<AuthorizationReceipt>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthorizationRequest {
    pub principal: Principal,
    pub action: CatalogAction,
    pub table: Option<TableIdent>,
    pub context: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogAction {
    CatalogConfig,
    NamespaceCreate,
    NamespaceList,
    TableCreate,
    TableRegister,
    TableLoad,
    TablePlanScan,
    TableCommit,
    TableDrop,
    TableRestore,
    CredentialsVend,
    WarehouseManage,
    StorageProfileManage,
    PolicyManage,
    GraphRead,
    LineageRead,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthorizationReceipt {
    pub principal: Principal,
    pub action: CatalogAction,
    pub table: Option<TableIdent>,
    pub allowed: bool,
    pub engine: String,
    pub policy_hash: Option<String>,
    pub context: Value,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct ReadRestriction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_columns: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_predicate: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_credential_ttl_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_hashes: Vec<String>,
}

impl ReadRestriction {
    pub fn unrestricted() -> Self {
        Self::default()
    }

    pub fn is_unrestricted(&self) -> bool {
        self.allowed_columns.is_none()
            && self.row_predicate.is_none()
            && self.purpose.is_none()
            && self.max_credential_ttl_seconds.is_none()
    }

    pub fn from_odrl_policies<'a>(
        policies: impl IntoIterator<Item = &'a Value>,
    ) -> LakeCatResult<Self> {
        let mut restriction = Self::unrestricted();
        for odrl in policies {
            let policy_hash = content_hash_json(odrl)?;
            restriction.policy_hashes.push(policy_hash);
            if let Some(columns) = allowed_columns_from_odrl(odrl)? {
                restriction.allowed_columns = Some(match restriction.allowed_columns.take() {
                    Some(existing) => intersect_columns(&existing, &columns),
                    None => columns,
                });
            }
            if let Some(row_predicate) = row_predicate_from_odrl(odrl)? {
                restriction.row_predicate = Some(match restriction.row_predicate.take() {
                    Some(existing) => and_row_predicates(existing, row_predicate),
                    None => row_predicate,
                });
            }
            if restriction.purpose.is_none() {
                restriction.purpose = purpose_from_odrl(odrl);
            }
            if let Some(ttl) = ttl_from_odrl(odrl)? {
                restriction.max_credential_ttl_seconds =
                    Some(match restriction.max_credential_ttl_seconds {
                        Some(existing) => existing.min(ttl),
                        None => ttl,
                    });
            }
        }
        Ok(restriction)
    }

    pub fn effective_projection(
        &self,
        requested_projection: &[String],
    ) -> LakeCatResult<Vec<String>> {
        let Some(allowed_columns) = self.allowed_columns.as_ref() else {
            return Ok(requested_projection.to_vec());
        };
        if allowed_columns.is_empty() {
            return Err(LakeCatError::Conflict(
                "read restriction leaves no columns available for scan planning".to_string(),
            ));
        }
        if requested_projection.is_empty() {
            return Ok(allowed_columns.clone());
        }
        let projection = requested_projection
            .iter()
            .filter(|column| allowed_columns.iter().any(|allowed| allowed == *column))
            .cloned()
            .collect::<Vec<_>>();
        if projection.is_empty() {
            return Err(LakeCatError::Conflict(
                "requested projection is outside the governed read restriction".to_string(),
            ));
        }
        Ok(projection)
    }

    pub fn effective_stats_fields(&self, requested: &[String]) -> Vec<String> {
        let Some(allowed_columns) = self.allowed_columns.as_ref() else {
            return requested.to_vec();
        };
        requested
            .iter()
            .filter(|column| allowed_columns.iter().any(|allowed| allowed == *column))
            .cloned()
            .collect()
    }

    pub fn mandatory_filters(&self) -> Vec<Value> {
        self.row_predicate.iter().cloned().collect()
    }
}

fn allowed_columns_from_odrl(odrl: &Value) -> LakeCatResult<Option<Vec<String>>> {
    for value in [
        odrl.get("allowed-columns"),
        odrl.get("allowedColumns"),
        odrl.get("columns"),
        odrl.get("lakecat:read-restriction")
            .and_then(|value| value.get("allowed-columns")),
        odrl.get("lakecat:read-restriction")
            .and_then(|value| value.get("allowedColumns")),
        odrl.get("read-restriction")
            .and_then(|value| value.get("allowed-columns")),
        odrl.get("readRestriction")
            .and_then(|value| value.get("allowedColumns")),
    ]
    .into_iter()
    .flatten()
    {
        return Ok(Some(string_list(value, "ODRL allowed columns")?));
    }

    let mut columns = Vec::new();
    for constraint in odrl_constraints(odrl) {
        let left = constraint
            .get("leftOperand")
            .or_else(|| constraint.get("left-operand"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if matches!(
            left,
            "column" | "columns" | "allowed-columns" | "allowedColumns" | "lakecat:allowed-columns"
        ) && let Some(value) = constraint
            .get("rightOperand")
            .or_else(|| constraint.get("right-operand"))
        {
            columns.extend(string_list(value, "ODRL allowed columns")?);
        }
    }
    if columns.is_empty() {
        Ok(None)
    } else {
        Ok(Some(dedup_columns(columns)))
    }
}

fn row_predicate_from_odrl(odrl: &Value) -> LakeCatResult<Option<Value>> {
    for value in [
        odrl.get("row-predicate"),
        odrl.get("rowPredicate"),
        odrl.get("lakecat:row-predicate"),
        odrl.get("lakecat:read-restriction")
            .and_then(|value| value.get("row-predicate")),
        odrl.get("lakecat:read-restriction")
            .and_then(|value| value.get("rowPredicate")),
        odrl.get("read-restriction")
            .and_then(|value| value.get("row-predicate")),
        odrl.get("readRestriction")
            .and_then(|value| value.get("rowPredicate")),
    ]
    .into_iter()
    .flatten()
    {
        return Ok(Some(row_predicate_value(value)?));
    }

    let mut predicate = None;
    for constraint in odrl_constraints(odrl) {
        let left = constraint
            .get("leftOperand")
            .or_else(|| constraint.get("left-operand"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if matches!(
            left,
            "row-predicate" | "rowPredicate" | "lakecat:row-predicate"
        ) && let Some(value) = constraint
            .get("rightOperand")
            .or_else(|| constraint.get("right-operand"))
        {
            let next = row_predicate_value(value)?;
            predicate = Some(match predicate {
                Some(existing) => and_row_predicates(existing, next),
                None => next,
            });
        }
    }
    Ok(predicate)
}

fn row_predicate_value(value: &Value) -> LakeCatResult<Value> {
    match value {
        Value::Object(_) => Ok(value.clone()),
        _ => Err(LakeCatError::InvalidArgument(
            "ODRL row predicate must be an Iceberg expression object".to_string(),
        )),
    }
}

fn and_row_predicates(left: Value, right: Value) -> Value {
    json!({
        "type": "and",
        "left": left,
        "right": right,
    })
}

fn purpose_from_odrl(odrl: &Value) -> Option<String> {
    odrl.get("purpose")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            odrl_constraints(odrl).into_iter().find_map(|constraint| {
                let left = constraint
                    .get("leftOperand")
                    .or_else(|| constraint.get("left-operand"))
                    .and_then(Value::as_str)?;
                if left == "purpose" {
                    constraint
                        .get("rightOperand")
                        .or_else(|| constraint.get("right-operand"))
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                } else {
                    None
                }
            })
        })
}

fn ttl_from_odrl(odrl: &Value) -> LakeCatResult<Option<u64>> {
    for value in [
        odrl.get("max-credential-ttl-seconds"),
        odrl.get("maxCredentialTtlSeconds"),
        odrl.get("lakecat:read-restriction")
            .and_then(|value| value.get("max-credential-ttl-seconds")),
        odrl.get("lakecat:read-restriction")
            .and_then(|value| value.get("maxCredentialTtlSeconds")),
        odrl.get("read-restriction")
            .and_then(|value| value.get("max-credential-ttl-seconds")),
        odrl.get("readRestriction")
            .and_then(|value| value.get("maxCredentialTtlSeconds")),
    ]
    .into_iter()
    .flatten()
    {
        return ttl_value(value);
    }

    for constraint in odrl_constraints(odrl) {
        let left = constraint
            .get("leftOperand")
            .or_else(|| constraint.get("left-operand"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if matches!(
            left,
            "max-credential-ttl-seconds"
                | "maxCredentialTtlSeconds"
                | "credential-ttl"
                | "credentialTtl"
                | "lakecat:max-credential-ttl-seconds"
        ) && let Some(value) = constraint
            .get("rightOperand")
            .or_else(|| constraint.get("right-operand"))
        {
            return ttl_value(value);
        }
    }
    Ok(None)
}

fn ttl_value(value: &Value) -> LakeCatResult<Option<u64>> {
    value.as_u64().map(Some).ok_or_else(|| {
        LakeCatError::InvalidArgument(
            "ODRL max credential TTL must be an unsigned integer number of seconds".to_string(),
        )
    })
}

fn odrl_constraints(odrl: &Value) -> Vec<&Value> {
    let mut constraints = Vec::new();
    for permission in values_as_slice(odrl.get("permission")) {
        constraints.extend(values_as_slice(permission.get("constraint")));
    }
    constraints.extend(values_as_slice(odrl.get("constraint")));
    constraints
}

fn values_as_slice(value: Option<&Value>) -> Vec<&Value> {
    match value {
        Some(Value::Array(values)) => values.iter().collect(),
        Some(value) => vec![value],
        None => Vec::new(),
    }
}

fn string_list(value: &Value, label: &str) -> LakeCatResult<Vec<String>> {
    let raw = match value {
        Value::Array(values) => values
            .iter()
            .map(|item| {
                item.as_str().map(ToString::to_string).ok_or_else(|| {
                    LakeCatError::InvalidArgument(format!("{label} must be strings"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        Value::String(value) => vec![value.clone()],
        _ => {
            return Err(LakeCatError::InvalidArgument(format!(
                "{label} must be a string or string array"
            )));
        }
    };
    Ok(dedup_columns(raw))
}

fn dedup_columns(columns: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(columns.len());
    for column in columns {
        if !out.iter().any(|existing| existing == &column) {
            out.push(column);
        }
    }
    out
}

fn intersect_columns(left: &[String], right: &[String]) -> Vec<String> {
    left.iter()
        .filter(|column| right.iter().any(|allowed| allowed == *column))
        .cloned()
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Capability<Action, Resource> {
    receipt: AuthorizationReceipt,
    resource: Resource,
    _action: std::marker::PhantomData<Action>,
}

impl<Action, Resource> Capability<Action, Resource> {
    pub fn receipt(&self) -> &AuthorizationReceipt {
        &self.receipt
    }

    pub fn resource(&self) -> &Resource {
        &self.resource
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanCreateTable;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanLoadTable;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanCommitTable;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanDropTable;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanRestoreTable;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanPlanScan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanVendCredentials;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanManageWarehouses;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanManageStorageProfiles;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanManagePolicies;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanReadGraph;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanReadLineage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanReadCatalogConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanCreateNamespace;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanListNamespaces;

pub type TableCreateCapability = Capability<CanCreateTable, TableIdent>;
pub type TableLoadCapability = Capability<CanLoadTable, TableIdent>;
pub type TableCommitCapability = Capability<CanCommitTable, TableIdent>;
pub type TableDropCapability = Capability<CanDropTable, TableIdent>;
pub type TableRestoreCapability = Capability<CanRestoreTable, TableIdent>;
pub type TableScanCapability = Capability<CanPlanScan, TableIdent>;
pub type CredentialsVendCapability = Capability<CanVendCredentials, TableIdent>;
pub type WarehouseManageCapability = Capability<CanManageWarehouses, ()>;
pub type StorageProfileManageCapability = Capability<CanManageStorageProfiles, ()>;
pub type PolicyManageCapability = Capability<CanManagePolicies, ()>;
pub type GraphReadCapability = Capability<CanReadGraph, ()>;
pub type LineageReadCapability = Capability<CanReadLineage, ()>;
pub type CatalogConfigCapability = Capability<CanReadCatalogConfig, ()>;
pub type NamespaceCreateCapability = Capability<CanCreateNamespace, ()>;
pub type NamespaceListCapability = Capability<CanListNamespaces, ()>;

impl TableCreateCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt, table: TableIdent) -> LakeCatResult<Self> {
        table_capability_from_receipt(receipt, table, CatalogAction::TableCreate, "create table")
    }

    pub fn table(&self) -> &TableIdent {
        self.resource()
    }
}

impl TableLoadCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt, table: TableIdent) -> LakeCatResult<Self> {
        table_capability_from_receipt(receipt, table, CatalogAction::TableLoad, "load table")
    }

    pub fn table(&self) -> &TableIdent {
        self.resource()
    }
}

impl TableCommitCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt, table: TableIdent) -> LakeCatResult<Self> {
        table_capability_from_receipt(receipt, table, CatalogAction::TableCommit, "commit table")
    }

    pub fn table(&self) -> &TableIdent {
        self.resource()
    }
}

impl TableDropCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt, table: TableIdent) -> LakeCatResult<Self> {
        table_capability_from_receipt(receipt, table, CatalogAction::TableDrop, "drop table")
    }

    pub fn table(&self) -> &TableIdent {
        self.resource()
    }
}

impl TableRestoreCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt, table: TableIdent) -> LakeCatResult<Self> {
        table_capability_from_receipt(receipt, table, CatalogAction::TableRestore, "restore table")
    }

    pub fn table(&self) -> &TableIdent {
        self.resource()
    }
}

impl TableScanCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt, table: TableIdent) -> LakeCatResult<Self> {
        table_capability_from_receipt(
            receipt,
            table,
            CatalogAction::TablePlanScan,
            "plan table scans",
        )
    }

    pub fn table(&self) -> &TableIdent {
        self.resource()
    }

    pub fn read_restriction(&self) -> LakeCatResult<ReadRestriction> {
        match self.receipt().context.get("read-restriction") {
            Some(value) => serde_json::from_value(value.clone()).map_err(|err| {
                LakeCatError::InvalidArgument(format!(
                    "authorization receipt carries invalid read restriction: {err}"
                ))
            }),
            None => Ok(ReadRestriction::unrestricted()),
        }
    }
}

impl CredentialsVendCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt, table: TableIdent) -> LakeCatResult<Self> {
        table_capability_from_receipt(
            receipt,
            table,
            CatalogAction::CredentialsVend,
            "vend table credentials",
        )
    }

    pub fn table(&self) -> &TableIdent {
        self.resource()
    }
}

impl StorageProfileManageCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(
            receipt,
            CatalogAction::StorageProfileManage,
            "manage storage profiles",
        )
    }
}

impl WarehouseManageCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(
            receipt,
            CatalogAction::WarehouseManage,
            "manage warehouses",
        )
    }
}

impl PolicyManageCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(receipt, CatalogAction::PolicyManage, "manage policies")
    }
}

impl GraphReadCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(receipt, CatalogAction::GraphRead, "read catalog graph")
    }
}

impl LineageReadCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(receipt, CatalogAction::LineageRead, "read catalog lineage")
    }
}

impl CatalogConfigCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(
            receipt,
            CatalogAction::CatalogConfig,
            "read catalog config",
        )
    }
}

impl NamespaceCreateCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(
            receipt,
            CatalogAction::NamespaceCreate,
            "create namespaces",
        )
    }
}

impl NamespaceListCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(receipt, CatalogAction::NamespaceList, "list namespaces")
    }
}

fn catalog_capability_from_receipt<Action>(
    receipt: AuthorizationReceipt,
    expected_action: CatalogAction,
    action_description: &str,
) -> LakeCatResult<Capability<Action, ()>> {
    if !receipt.allowed {
        return Err(LakeCatError::Conflict(
            "authorization receipt is not allowed".to_string(),
        ));
    }
    if receipt.action != expected_action {
        return Err(LakeCatError::InvalidArgument(format!(
            "authorization receipt action {:?} cannot {action_description}",
            receipt.action,
        )));
    }
    if receipt.table.is_some() {
        return Err(LakeCatError::InvalidArgument(
            "catalog authorization receipt must not be table-scoped".to_string(),
        ));
    }
    Ok(Capability {
        receipt,
        resource: (),
        _action: std::marker::PhantomData,
    })
}

fn table_capability_from_receipt<Action>(
    receipt: AuthorizationReceipt,
    table: TableIdent,
    expected_action: CatalogAction,
    action_description: &str,
) -> LakeCatResult<Capability<Action, TableIdent>> {
    if !receipt.allowed {
        return Err(LakeCatError::Conflict(
            "authorization receipt is not allowed".to_string(),
        ));
    }
    if receipt.action != expected_action {
        return Err(LakeCatError::InvalidArgument(format!(
            "authorization receipt action {:?} cannot {action_description}",
            receipt.action,
        )));
    }
    if receipt.table.as_ref() != Some(&table) {
        return Err(LakeCatError::InvalidArgument(
            "authorization receipt table does not match scan table".to_string(),
        ));
    }
    Ok(Capability {
        receipt,
        resource: table,
        _action: std::marker::PhantomData,
    })
}

#[cfg(test)]
mod tests {
    use lakecat_core::{Namespace, PrincipalKind, TableName, WarehouseName};

    use super::*;

    #[test]
    fn read_restriction_composes_odrl_policy_documents() {
        let policy_a = serde_json::json!({
            "uid": "policy-a",
            "purpose": "resilience-demo",
            "lakecat:read-restriction": {
                "max-credential-ttl-seconds": 900,
                "allowed-columns": ["event_id", "payload"],
                "row-predicate": {
                    "type": "equal",
                    "term": "region",
                    "value": "west"
                }
            }
        });
        let policy_b = serde_json::json!({
            "uid": "policy-b",
            "max-credential-ttl-seconds": 300,
            "permission": [{
                "constraint": [
                    {
                        "leftOperand": "allowed-columns",
                        "operator": "eq",
                        "rightOperand": ["event_id", "severity"]
                    },
                    {
                        "leftOperand": "row-predicate",
                        "operator": "eq",
                        "rightOperand": {
                            "type": "greater-than-or-equal",
                            "term": "severity",
                            "value": 3
                        }
                    }
                ]
            }]
        });

        let restriction = ReadRestriction::from_odrl_policies([&policy_a, &policy_b]).unwrap();

        assert_eq!(
            restriction.allowed_columns,
            Some(vec!["event_id".to_string()])
        );
        assert_eq!(restriction.purpose.as_deref(), Some("resilience-demo"));
        assert_eq!(restriction.max_credential_ttl_seconds, Some(300));
        assert_eq!(restriction.policy_hashes.len(), 2);
        assert_eq!(
            restriction.row_predicate,
            Some(serde_json::json!({
                "type": "and",
                "left": {
                    "type": "equal",
                    "term": "region",
                    "value": "west"
                },
                "right": {
                    "type": "greater-than-or-equal",
                    "term": "severity",
                    "value": 3
                }
            }))
        );
    }

    #[test]
    fn read_restriction_parses_ttl_from_odrl_constraints_and_uses_tightest_ttl() {
        let policy_a = serde_json::json!({
            "uid": "policy-a",
            "lakecat:read-restriction": {
                "max-credential-ttl-seconds": 900
            }
        });
        let policy_b = serde_json::json!({
            "uid": "policy-b",
            "permission": [{
                "constraint": {
                    "leftOperand": "max-credential-ttl-seconds",
                    "operator": "lteq",
                    "rightOperand": 300
                }
            }]
        });

        let restriction = ReadRestriction::from_odrl_policies([&policy_a, &policy_b]).unwrap();

        assert_eq!(restriction.max_credential_ttl_seconds, Some(300));
    }

    #[test]
    fn read_restriction_rejects_non_numeric_ttl_constraints() {
        let policy = serde_json::json!({
            "permission": [{
                "constraint": {
                    "leftOperand": "credential-ttl",
                    "operator": "lteq",
                    "rightOperand": "five minutes"
                }
            }]
        });

        let err = ReadRestriction::from_odrl_policies([&policy]).unwrap_err();

        assert!(
            err.to_string()
                .contains("ODRL max credential TTL must be an unsigned integer")
        );
    }

    #[test]
    fn read_restriction_rejects_non_object_row_predicates() {
        let policy = serde_json::json!({
            "read-restriction": {
                "row-predicate": "severity >= 3"
            }
        });

        let err = ReadRestriction::from_odrl_policies([&policy]).unwrap_err();

        assert!(
            err.to_string()
                .contains("ODRL row predicate must be an Iceberg expression object")
        );
    }

    #[test]
    fn read_restriction_narrows_projection_stats_and_filters() {
        let row_predicate = serde_json::json!({
            "type": "equal",
            "term": "region",
            "value": "west"
        });
        let restriction = ReadRestriction {
            allowed_columns: Some(vec!["event_id".to_string(), "severity".to_string()]),
            row_predicate: Some(row_predicate.clone()),
            ..ReadRestriction::unrestricted()
        };

        assert_eq!(
            restriction.effective_projection(&[]).unwrap(),
            vec!["event_id".to_string(), "severity".to_string()]
        );
        assert_eq!(
            restriction
                .effective_projection(&["event_id".to_string(), "payload".to_string()])
                .unwrap(),
            vec!["event_id".to_string()]
        );
        assert!(
            restriction
                .effective_projection(&["payload".to_string()])
                .is_err()
        );
        assert_eq!(
            restriction.effective_stats_fields(&["payload".to_string(), "severity".to_string()]),
            vec!["severity".to_string()]
        );
        assert_eq!(restriction.mandatory_filters(), vec![row_predicate]);
    }

    #[test]
    fn table_capabilities_require_matching_allowed_receipts() {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:reader".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::TablePlanScan,
            table: Some(table.clone()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: Utc::now(),
        };

        let capability = TableScanCapability::from_receipt(receipt.clone(), table.clone())
            .expect("matching receipt should mint capability");
        assert_eq!(capability.table(), &table);
        assert_eq!(capability.receipt(), &receipt);

        let other_table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("other").unwrap(),
        );
        assert!(TableScanCapability::from_receipt(receipt.clone(), other_table).is_err());

        let mut wrong_action_receipt = receipt;
        wrong_action_receipt.action = CatalogAction::TableLoad;
        assert!(TableScanCapability::from_receipt(wrong_action_receipt, table.clone()).is_err());

        let load_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:reader".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::TableLoad,
            table: Some(capability.table().clone()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: Utc::now(),
        };
        let load_capability =
            TableLoadCapability::from_receipt(load_receipt, capability.table().clone())
                .expect("matching load receipt should mint capability");
        assert_eq!(load_capability.table(), capability.table());

        let commit_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:writer".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::TableCommit,
            table: Some(capability.table().clone()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: Utc::now(),
        };
        let commit_capability =
            TableCommitCapability::from_receipt(commit_receipt, capability.table().clone())
                .expect("matching commit receipt should mint capability");
        assert_eq!(commit_capability.table(), capability.table());

        let drop_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:writer".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::TableDrop,
            table: Some(capability.table().clone()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: Utc::now(),
        };
        let drop_capability =
            TableDropCapability::from_receipt(drop_receipt, capability.table().clone())
                .expect("matching drop receipt should mint capability");
        assert_eq!(drop_capability.table(), capability.table());

        let create_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:writer".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::TableCreate,
            table: Some(capability.table().clone()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: Utc::now(),
        };
        let create_capability =
            TableCreateCapability::from_receipt(create_receipt, capability.table().clone())
                .expect("matching create receipt should mint capability");
        assert_eq!(create_capability.table(), capability.table());

        let credentials_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:reader".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::CredentialsVend,
            table: Some(capability.table().clone()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: Utc::now(),
        };
        let credentials_capability = CredentialsVendCapability::from_receipt(
            credentials_receipt,
            capability.table().clone(),
        )
        .expect("matching credential receipt should mint capability");
        assert_eq!(credentials_capability.table(), capability.table());

        let graph_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:querygraph".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::GraphRead,
            table: None,
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: Utc::now(),
        };
        let graph_capability = GraphReadCapability::from_receipt(graph_receipt.clone())
            .expect("matching graph-read receipt should mint capability");
        assert_eq!(graph_capability.receipt(), &graph_receipt);

        let mut table_scoped_graph_receipt = graph_receipt;
        table_scoped_graph_receipt.table = Some(capability.table().clone());
        assert!(GraphReadCapability::from_receipt(table_scoped_graph_receipt).is_err());

        let config_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:catalog".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::CatalogConfig,
            table: None,
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: Utc::now(),
        };
        assert!(CatalogConfigCapability::from_receipt(config_receipt).is_ok());

        let namespace_create_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:catalog".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::NamespaceCreate,
            table: None,
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: Utc::now(),
        };
        assert!(NamespaceCreateCapability::from_receipt(namespace_create_receipt).is_ok());

        let namespace_list_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:catalog".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::NamespaceList,
            table: None,
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: Utc::now(),
        };
        assert!(NamespaceListCapability::from_receipt(namespace_list_receipt).is_ok());

        let warehouse_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:operator".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::WarehouseManage,
            table: None,
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: Utc::now(),
        };
        assert!(WarehouseManageCapability::from_receipt(warehouse_receipt).is_ok());

        let storage_profile_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:operator".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::StorageProfileManage,
            table: None,
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: Utc::now(),
        };
        assert!(StorageProfileManageCapability::from_receipt(storage_profile_receipt).is_ok());

        let policy_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:operator".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::PolicyManage,
            table: None,
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: Utc::now(),
        };
        assert!(PolicyManageCapability::from_receipt(policy_receipt).is_ok());

        let restore_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:operator".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::TableRestore,
            table: Some(table.clone()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: Utc::now(),
        };
        assert!(TableRestoreCapability::from_receipt(restore_receipt, table).is_ok());
    }
}

#[derive(Debug, Default)]
pub struct AllowAllGovernanceEngine;

impl AllowAllGovernanceEngine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl GovernanceEngine for AllowAllGovernanceEngine {
    async fn authorize(
        &self,
        request: AuthorizationRequest,
    ) -> LakeCatResult<AuthorizationReceipt> {
        Ok(AuthorizationReceipt {
            principal: request.principal,
            action: request.action,
            table: request.table,
            allowed: true,
            engine: "lakecat-allow-all-typesec-placeholder".to_string(),
            policy_hash: None,
            context: request.context,
            checked_at: Utc::now(),
        })
    }
}

#[cfg(feature = "typesec-local")]
pub mod typesec_integration {
    use std::sync::Arc;

    use async_trait::async_trait;
    use chrono::Utc;
    use lakecat_core::{LakeCatResult, content_hash_json};
    use typesec::{
        CombineStrategy, ComposedEngine, PolicyEngine, PolicyResult, ResourceId, SubjectId,
    };

    use crate::{
        AuthorizationReceipt, AuthorizationRequest, GovernanceEngine, action_name, resource_name,
    };

    pub struct TypeSecGovernanceEngine {
        engine: Arc<dyn PolicyEngine>,
    }

    impl TypeSecGovernanceEngine {
        pub fn new(engine: Arc<dyn PolicyEngine>) -> Arc<Self> {
            Arc::new(Self { engine })
        }

        pub fn with_fallback(
            primary: Arc<dyn PolicyEngine>,
            fallback: Arc<dyn PolicyEngine>,
        ) -> Arc<Self> {
            Arc::new(Self {
                engine: Arc::new(ComposedEngine::new(
                    vec![primary, fallback],
                    CombineStrategy::PriorityOrder,
                )),
            })
        }

        pub fn allow_all() -> Arc<Self> {
            Arc::new(Self {
                engine: Arc::new(AllowAllPolicy),
            })
        }

        pub fn rbac_from_yaml(yaml: &str) -> LakeCatResult<Arc<Self>> {
            let engine = typesec::RbacEngine::from_yaml(yaml).map_err(|err| {
                lakecat_core::LakeCatError::InvalidArgument(format!(
                    "failed to load TypeSec RBAC policy: {err}"
                ))
            })?;
            Ok(Self::new(Arc::new(engine)))
        }
    }

    struct AllowAllPolicy;

    impl PolicyEngine for AllowAllPolicy {
        fn check(
            &self,
            _subject: &SubjectId,
            _action: &str,
            _resource: &ResourceId,
        ) -> PolicyResult {
            PolicyResult::Allow
        }
    }

    #[async_trait]
    impl GovernanceEngine for TypeSecGovernanceEngine {
        async fn authorize(
            &self,
            request: AuthorizationRequest,
        ) -> LakeCatResult<AuthorizationReceipt> {
            let subject = SubjectId::from(request.principal.subject.clone());
            let action = action_name(&request.action);
            let resource = ResourceId::from(resource_name(&request));
            let decision = self.engine.check(&subject, action, &resource);
            let allowed = matches!(decision, PolicyResult::Allow);
            Ok(AuthorizationReceipt {
                principal: request.principal,
                action: request.action,
                table: request.table,
                allowed,
                engine: "typesec".to_string(),
                policy_hash: Some(content_hash_json(&serde_json::json!({
                    "engine": "typesec",
                    "subject": subject.as_str(),
                    "action": action,
                    "resource": resource.as_str(),
                    "decision": policy_result_name(&decision),
                    "context-hash": content_hash_json(&request.context)?,
                }))?),
                context: request.context,
                checked_at: Utc::now(),
            })
        }
    }

    fn policy_result_name(result: &PolicyResult) -> &'static str {
        match result {
            PolicyResult::Allow => "allow",
            PolicyResult::Deny(_) => "deny",
            PolicyResult::Delegate(_) => "delegate",
            _ => "unknown",
        }
    }

    #[cfg(test)]
    mod tests {
        use std::sync::Arc;

        use lakecat_core::{Principal, PrincipalKind};
        use typesec::{PolicyEngine, PolicyResult, ResourceId, SubjectId};

        use super::*;
        use crate::{AuthorizationRequest, CatalogAction};

        struct AllowRead;
        struct DelegateToRbac;

        impl PolicyEngine for AllowRead {
            fn check(
                &self,
                _subject: &SubjectId,
                action: &str,
                _resource: &ResourceId,
            ) -> PolicyResult {
                if action == "table.load" {
                    PolicyResult::Allow
                } else {
                    PolicyResult::Deny("not granted".to_string())
                }
            }
        }

        impl PolicyEngine for DelegateToRbac {
            fn check(
                &self,
                _subject: &SubjectId,
                _action: &str,
                _resource: &ResourceId,
            ) -> PolicyResult {
                PolicyResult::delegate("odrl", "rbac decides base access")
            }
        }

        #[tokio::test]
        async fn delegates_authorization_to_typesec_policy_engine() {
            let engine = TypeSecGovernanceEngine::new(Arc::new(AllowRead));
            let receipt = engine
                .authorize(AuthorizationRequest {
                    principal: Principal {
                        subject: "agent:reader".to_string(),
                        kind: PrincipalKind::Agent,
                    },
                    action: CatalogAction::TableLoad,
                    table: None,
                    context: serde_json::json!({}),
                })
                .await
                .expect("authorization should run");
            assert!(receipt.allowed);
            assert_eq!(receipt.engine, "typesec");
            assert!(receipt.policy_hash.is_some());
        }

        #[tokio::test]
        async fn delegates_to_typesec_fallback_policy_engine() {
            let engine = TypeSecGovernanceEngine::with_fallback(
                Arc::new(DelegateToRbac),
                Arc::new(AllowRead),
            );
            let receipt = engine
                .authorize(AuthorizationRequest {
                    principal: Principal {
                        subject: "agent:reader".to_string(),
                        kind: PrincipalKind::Agent,
                    },
                    action: CatalogAction::TableLoad,
                    table: None,
                    context: serde_json::json!({"read-restriction": {"allowed-columns": ["id"]}}),
                })
                .await
                .expect("authorization should run through TypeSec fallback");
            assert!(receipt.allowed);
            assert_eq!(receipt.engine, "typesec");
            assert!(receipt.policy_hash.is_some());
            assert_eq!(
                receipt.context["read-restriction"]["allowed-columns"][0],
                serde_json::json!("id")
            );
        }

        #[tokio::test]
        async fn loads_rbac_policy_yaml_for_authorization() {
            let engine = TypeSecGovernanceEngine::rbac_from_yaml(
                r#"
roles:
  - name: scanner
    permissions: ["table.plan_scan"]
    resources: ["lakecat:table:local:default:events"]
assignments:
  - subject: "agent:scanner"
    roles: [scanner]
"#,
            )
            .expect("rbac policy should load");
            let table = lakecat_core::TableIdent::new(
                lakecat_core::WarehouseName::new("local").unwrap(),
                "default".parse::<lakecat_core::Namespace>().unwrap(),
                lakecat_core::TableName::new("events").unwrap(),
            );
            let receipt = engine
                .authorize(AuthorizationRequest {
                    principal: lakecat_core::Principal::new(
                        "agent:scanner",
                        lakecat_core::PrincipalKind::Agent,
                    )
                    .unwrap(),
                    action: CatalogAction::TablePlanScan,
                    table: Some(table),
                    context: serde_json::json!({}),
                })
                .await
                .unwrap();

            assert!(receipt.allowed);
            assert_eq!(receipt.engine, "typesec");
        }

        #[test]
        fn rejects_invalid_rbac_policy_yaml() {
            let error = match TypeSecGovernanceEngine::rbac_from_yaml(
                r#"
roles:
  - name: broken
    inherits: [missing]
"#,
            ) {
                Ok(_) => panic!("invalid rbac policy should fail closed"),
                Err(error) => error,
            };

            assert!(
                error
                    .to_string()
                    .contains("failed to load TypeSec RBAC policy")
            );
        }
    }
}

pub fn action_name(action: &CatalogAction) -> &'static str {
    match action {
        CatalogAction::CatalogConfig => "catalog.config",
        CatalogAction::NamespaceCreate => "namespace.create",
        CatalogAction::NamespaceList => "namespace.list",
        CatalogAction::TableCreate => "table.create",
        CatalogAction::TableRegister => "table.register",
        CatalogAction::TableLoad => "table.load",
        CatalogAction::TablePlanScan => "table.plan_scan",
        CatalogAction::TableCommit => "table.commit",
        CatalogAction::TableDrop => "table.drop",
        CatalogAction::TableRestore => "table.restore",
        CatalogAction::CredentialsVend => "credentials.vend",
        CatalogAction::WarehouseManage => "warehouse.manage",
        CatalogAction::StorageProfileManage => "storage_profile.manage",
        CatalogAction::PolicyManage => "policy.manage",
        CatalogAction::GraphRead => "graph.read",
        CatalogAction::LineageRead => "lineage.read",
    }
}

pub fn resource_name(request: &AuthorizationRequest) -> String {
    request
        .table
        .as_ref()
        .map(TableIdent::stable_id)
        .unwrap_or_else(|| "lakecat:catalog".to_string())
}
