pub mod sail;

use std::fmt::{Display, Formatter};
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub type LakeCatResult<T> = Result<T, LakeCatError>;

#[derive(Debug, thiserror::Error)]
pub enum LakeCatError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("{object} not found: {name}")]
    NotFound { object: &'static str, name: String },
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("operation is not supported yet: {0}")]
    NotSupported(String),
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(pub Uuid);

impl ProjectId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ProjectId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for ProjectId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WarehouseName(String);

impl WarehouseName {
    pub fn new(value: impl Into<String>) -> LakeCatResult<Self> {
        let value = value.into();
        validate_name("warehouse", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for WarehouseName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Namespace(Vec<String>);

impl Namespace {
    pub fn new(parts: Vec<String>) -> LakeCatResult<Self> {
        if parts.is_empty() {
            return Err(LakeCatError::InvalidArgument(
                "namespace must contain at least one component".to_string(),
            ));
        }
        for part in &parts {
            validate_name("namespace component", part)?;
        }
        Ok(Self(parts))
    }

    pub fn root_default() -> Self {
        Self(vec!["default".to_string()])
    }

    pub fn parts(&self) -> &[String] {
        &self.0
    }

    pub fn path(&self) -> String {
        self.0.join(".")
    }
}

impl FromStr for Namespace {
    type Err = LakeCatError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parts = value
            .split('.')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        Self::new(parts)
    }
}

impl Display for Namespace {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.path())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableName(String);

impl TableName {
    pub fn new(value: impl Into<String>) -> LakeCatResult<Self> {
        let value = value.into();
        validate_name("table", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for TableName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableIdent {
    pub warehouse: WarehouseName,
    pub namespace: Namespace,
    pub name: TableName,
}

impl TableIdent {
    pub fn new(warehouse: WarehouseName, namespace: Namespace, name: TableName) -> Self {
        Self {
            warehouse,
            namespace,
            name,
        }
    }

    pub fn stable_id(&self) -> String {
        format!(
            "lakecat:table:{}:{}:{}",
            self.warehouse, self.namespace, self.name
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    pub subject: String,
    pub kind: PrincipalKind,
}

impl Principal {
    pub fn anonymous() -> Self {
        Self {
            subject: "anonymous".to_string(),
            kind: PrincipalKind::Anonymous,
        }
    }

    pub fn new(subject: impl Into<String>, kind: PrincipalKind) -> LakeCatResult<Self> {
        let subject = subject.into();
        if subject.trim().is_empty() {
            return Err(LakeCatError::InvalidArgument(
                "principal subject must not be empty".to_string(),
            ));
        }
        Ok(Self { subject, kind })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrincipalKind {
    Anonymous,
    Human,
    Service,
    Agent,
}

impl FromStr for PrincipalKind {
    type Err = LakeCatError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "anonymous" => Ok(Self::Anonymous),
            "human" | "user" => Ok(Self::Human),
            "service" | "service-account" => Ok(Self::Service),
            "agent" | "typedid-agent" | "typedid" | "did" => Ok(Self::Agent),
            other => Err(LakeCatError::InvalidArgument(format!(
                "unknown principal kind: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditStamp {
    pub principal: Principal,
    pub at: DateTime<Utc>,
}

impl AuditStamp {
    pub fn now(principal: Principal) -> Self {
        Self {
            principal,
            at: Utc::now(),
        }
    }
}

pub fn content_hash_json(value: &serde_json::Value) -> LakeCatResult<String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|err| LakeCatError::Internal(format!("failed to encode JSON: {err}")))?;
    Ok(content_hash_bytes(&bytes))
}

pub fn content_hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("sha256:{}", hex::encode(digest))
}

fn validate_name(kind: &str, value: &str) -> LakeCatResult<()> {
    if value.is_empty() {
        return Err(LakeCatError::InvalidArgument(format!(
            "{kind} name must not be empty"
        )));
    }
    let valid = value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'));
    if !valid {
        return Err(LakeCatError::InvalidArgument(format!(
            "{kind} name contains unsupported characters: {value}"
        )));
    }
    Ok(())
}
