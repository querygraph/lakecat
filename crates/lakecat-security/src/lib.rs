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

pub const ALLOW_ALL_LOCAL_ENGINE: &str = "lakecat-allow-all-local";

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
    NamespaceLoad,
    NamespaceDrop,
    TableCreate,
    TableRegister,
    TableLoad,
    TablePlanScan,
    TableCommit,
    TableDrop,
    TableRestore,
    CredentialsVend,
    ViewLoad,
    ViewDrop,
    ServerManage,
    ProjectManage,
    WarehouseManage,
    StorageProfileManage,
    ViewManage,
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

impl AuthorizationReceipt {
    pub fn with_read_restriction_policy_hash(mut self) -> LakeCatResult<Self> {
        let Some(restriction) = self.context.get("read-restriction") else {
            return Ok(self);
        };
        let restriction: ReadRestriction =
            serde_json::from_value(restriction.clone()).map_err(|err| {
                LakeCatError::InvalidArgument(format!(
                    "authorization receipt carries invalid read restriction: {err}"
                ))
            })?;
        if restriction.policy_hashes.is_empty() {
            return Ok(self);
        }
        self.policy_hash = Some(content_hash_json(&json!({
            "engine": self.engine,
            "governance-policy-hash": self.policy_hash,
            "read-restriction-policy-hashes": restriction.policy_hashes,
            "action": self.action,
            "table": self.table,
        }))?);
        Ok(self)
    }
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

    pub fn requires_governed_read(&self) -> bool {
        self.allowed_columns.is_some() || self.row_predicate.is_some()
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
            if let Some(purpose) = purpose_from_odrl(odrl)? {
                restriction.purpose = Some(match restriction.purpose.take() {
                    Some(existing) if existing == purpose => existing,
                    Some(existing) => {
                        return Err(LakeCatError::Conflict(format!(
                            "ODRL read restriction carries conflicting purposes {existing} and {purpose}"
                        )));
                    }
                    None => purpose,
                });
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
        let left = constraint_left_operand(constraint).unwrap_or_default();
        if matches!(
            left,
            "column" | "columns" | "allowed-columns" | "allowedColumns" | "lakecat:allowed-columns"
        ) {
            let value = constraint_right_operand(constraint, "allowed columns")?;
            require_constraint_operator(
                constraint,
                "allowed columns",
                &["eq", "isAnyOf", "isAllOf"],
            )?;
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
        let left = constraint_left_operand(constraint).unwrap_or_default();
        if matches!(
            left,
            "row-predicate" | "rowPredicate" | "lakecat:row-predicate"
        ) {
            let value = constraint_right_operand(constraint, "row predicate")?;
            require_constraint_operator(constraint, "row predicate", &["eq"])?;
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

fn purpose_from_odrl(odrl: &Value) -> LakeCatResult<Option<String>> {
    let mut purpose = odrl
        .get("purpose")
        .map(|value| nonblank_jsonld_string(value, "ODRL purpose"))
        .transpose()?
        .map(str::to_string);

    for constraint in odrl_constraints(odrl) {
        let left = constraint_left_operand(constraint).unwrap_or_default();
        if matches!(left, "purpose" | "lakecat:purpose") {
            require_constraint_operator(constraint, "purpose", &["eq"])?;
            let right_operand = constraint_right_operand(constraint, "purpose")?;
            let next = jsonld_string_value(right_operand)
                .ok_or_else(|| {
                    LakeCatError::InvalidArgument(
                        "ODRL purpose constraint must use a string right operand".to_string(),
                    )
                })
                .and_then(|value| require_nonblank_string(value, "ODRL purpose constraint"))?
                .to_string();
            purpose = Some(match purpose.take() {
                Some(existing) if existing == next => existing,
                Some(existing) => {
                    return Err(LakeCatError::Conflict(format!(
                        "ODRL read restriction carries conflicting purposes {existing} and {next}"
                    )));
                }
                None => next,
            });
        }
    }
    Ok(purpose)
}

fn ttl_from_odrl(odrl: &Value) -> LakeCatResult<Option<u64>> {
    let mut ttl = None;
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
        ttl = tighten_ttl(ttl, ttl_value(value)?);
    }

    for constraint in odrl_constraints(odrl) {
        let left = constraint_left_operand(constraint).unwrap_or_default();
        if matches!(
            left,
            "max-credential-ttl-seconds"
                | "maxCredentialTtlSeconds"
                | "credential-ttl"
                | "credentialTtl"
                | "lakecat:max-credential-ttl-seconds"
                | "lakecat:credential-ttl"
        ) {
            let value = constraint_right_operand(constraint, "max credential TTL")?;
            require_constraint_operator(constraint, "max credential TTL", &["eq", "lteq", "lt"])?;
            ttl = tighten_ttl(ttl, ttl_value(value)?);
        }
    }
    Ok(ttl)
}

fn tighten_ttl(existing: Option<u64>, next: Option<u64>) -> Option<u64> {
    match (existing, next) {
        (Some(existing), Some(next)) => Some(existing.min(next)),
        (Some(existing), None) => Some(existing),
        (None, next) => next,
    }
}

fn require_constraint_operator(
    constraint: &Value,
    label: &str,
    allowed: &[&str],
) -> LakeCatResult<()> {
    let Some(operator) = constraint_operator(constraint) else {
        return Err(LakeCatError::InvalidArgument(format!(
            "ODRL {label} constraint must include an operator"
        )));
    };
    let normalized = operator
        .strip_prefix("odrl:")
        .or_else(|| operator.strip_prefix("http://www.w3.org/ns/odrl/2/"))
        .unwrap_or(operator);
    if allowed.iter().any(|allowed| allowed == &normalized) {
        Ok(())
    } else {
        Err(LakeCatError::InvalidArgument(format!(
            "ODRL {label} constraint uses unsupported operator {operator}"
        )))
    }
}

fn constraint_operator(constraint: &Value) -> Option<&str> {
    constraint
        .get("operator")
        .or_else(|| constraint.get("odrl:operator"))
        .and_then(jsonld_term)
}

fn constraint_left_operand(constraint: &Value) -> Option<&str> {
    constraint
        .get("leftOperand")
        .or_else(|| constraint.get("left-operand"))
        .or_else(|| constraint.get("odrl:leftOperand"))
        .and_then(jsonld_term)
}

fn jsonld_term(value: &Value) -> Option<&str> {
    value
        .as_str()
        .or_else(|| value.get("@id").and_then(Value::as_str))
}

fn constraint_right_operand<'a>(constraint: &'a Value, label: &str) -> LakeCatResult<&'a Value> {
    constraint
        .get("rightOperand")
        .or_else(|| constraint.get("right-operand"))
        .or_else(|| constraint.get("odrl:rightOperand"))
        .ok_or_else(|| {
            LakeCatError::InvalidArgument(format!(
                "ODRL {label} constraint must include a right operand"
            ))
        })
}

fn ttl_value(value: &Value) -> LakeCatResult<Option<u64>> {
    jsonld_u64_value(value).map(Some).ok_or_else(|| {
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
            .map(|item| nonblank_jsonld_string(item, label).map(ToString::to_string))
            .collect::<Result<Vec<_>, _>>()?,
        Value::Object(object) if object.contains_key("@list") => object
            .get("@list")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                LakeCatError::InvalidArgument(format!("{label} @list must be an array"))
            })?
            .iter()
            .map(|item| nonblank_jsonld_string(item, label).map(ToString::to_string))
            .collect::<Result<Vec<_>, _>>()?,
        value if jsonld_string_value(value).is_some() => {
            vec![nonblank_jsonld_string(value, label)?.to_string()]
        }
        _ => {
            return Err(LakeCatError::InvalidArgument(format!(
                "{label} must be a string or string array"
            )));
        }
    };
    if raw.is_empty() {
        return Err(LakeCatError::InvalidArgument(format!(
            "{label} must not be empty"
        )));
    }
    Ok(dedup_columns(raw))
}

fn nonblank_jsonld_string<'a>(value: &'a Value, label: &str) -> LakeCatResult<&'a str> {
    let value = jsonld_string_value(value)
        .ok_or_else(|| LakeCatError::InvalidArgument(format!("{label} must be strings")))?;
    require_nonblank_string(value, label)
}

fn require_nonblank_string<'a>(value: &'a str, label: &str) -> LakeCatResult<&'a str> {
    if value.trim().is_empty() {
        return Err(LakeCatError::InvalidArgument(format!(
            "{label} must not be blank"
        )));
    }
    Ok(value)
}

fn jsonld_string_value(value: &Value) -> Option<&str> {
    value
        .as_str()
        .or_else(|| value.get("@value").and_then(Value::as_str))
}

fn jsonld_u64_value(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value.get("@value").and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|value| value.parse::<u64>().ok()))
        })
    })
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
pub struct CanManageServers;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanManageProjects;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanManageWarehouses;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanManageStorageProfiles;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanLoadViews;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanDropViews;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanManageViews;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanLoadNamespace;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanDropNamespace;

pub type TableCreateCapability = Capability<CanCreateTable, TableIdent>;
pub type TableLoadCapability = Capability<CanLoadTable, TableIdent>;
pub type TableCommitCapability = Capability<CanCommitTable, TableIdent>;
pub type TableDropCapability = Capability<CanDropTable, TableIdent>;
pub type TableRestoreCapability = Capability<CanRestoreTable, TableIdent>;
pub type TableScanCapability = Capability<CanPlanScan, TableIdent>;
pub type CredentialsVendCapability = Capability<CanVendCredentials, TableIdent>;
pub type ServerManageCapability = Capability<CanManageServers, ()>;
pub type ProjectManageCapability = Capability<CanManageProjects, ()>;
pub type WarehouseManageCapability = Capability<CanManageWarehouses, ()>;
pub type StorageProfileManageCapability = Capability<CanManageStorageProfiles, ()>;
pub type ViewLoadCapability = Capability<CanLoadViews, ()>;
pub type ViewDropCapability = Capability<CanDropViews, ()>;
pub type ViewManageCapability = Capability<CanManageViews, ()>;
pub type PolicyManageCapability = Capability<CanManagePolicies, ()>;
pub type GraphReadCapability = Capability<CanReadGraph, ()>;
pub type LineageReadCapability = Capability<CanReadLineage, ()>;
pub type CatalogConfigCapability = Capability<CanReadCatalogConfig, ()>;
pub type NamespaceCreateCapability = Capability<CanCreateNamespace, ()>;
pub type NamespaceListCapability = Capability<CanListNamespaces, ()>;
pub type NamespaceLoadCapability = Capability<CanLoadNamespace, ()>;
pub type NamespaceDropCapability = Capability<CanDropNamespace, ()>;

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

impl ServerManageCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(receipt, CatalogAction::ServerManage, "manage servers")
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

impl ViewManageCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(receipt, CatalogAction::ViewManage, "manage views")
    }
}

impl ViewLoadCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(receipt, CatalogAction::ViewLoad, "load views")
    }
}

impl ViewDropCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(receipt, CatalogAction::ViewDrop, "drop views")
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

impl ProjectManageCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(receipt, CatalogAction::ProjectManage, "manage projects")
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

impl NamespaceLoadCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(receipt, CatalogAction::NamespaceLoad, "load namespaces")
    }
}

impl NamespaceDropCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(receipt, CatalogAction::NamespaceDrop, "drop namespaces")
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
mod tests;

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
            engine: ALLOW_ALL_LOCAL_ENGINE.to_string(),
            policy_hash: None,
            context: request.context,
            checked_at: Utc::now(),
        })
    }
}

#[cfg(feature = "typesec-local")]
pub mod typesec_integration;

pub fn action_name(action: &CatalogAction) -> &'static str {
    match action {
        CatalogAction::CatalogConfig => "catalog.config",
        CatalogAction::NamespaceCreate => "namespace.create",
        CatalogAction::NamespaceList => "namespace.list",
        CatalogAction::NamespaceLoad => "namespace.load",
        CatalogAction::NamespaceDrop => "namespace.drop",
        CatalogAction::TableCreate => "table.create",
        CatalogAction::TableRegister => "table.register",
        CatalogAction::TableLoad => "table.load",
        CatalogAction::TablePlanScan => "table.plan_scan",
        CatalogAction::TableCommit => "table.commit",
        CatalogAction::TableDrop => "table.drop",
        CatalogAction::TableRestore => "table.restore",
        CatalogAction::CredentialsVend => "credentials.vend",
        CatalogAction::ViewLoad => "view.load",
        CatalogAction::ViewDrop => "view.drop",
        CatalogAction::ServerManage => "server.manage",
        CatalogAction::ProjectManage => "project.manage",
        CatalogAction::WarehouseManage => "warehouse.manage",
        CatalogAction::StorageProfileManage => "storage_profile.manage",
        CatalogAction::ViewManage => "view.manage",
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
