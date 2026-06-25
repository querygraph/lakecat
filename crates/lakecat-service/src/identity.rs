use axum::http::HeaderMap;
use lakecat_core::{LakeCatError, Principal, PrincipalKind, TableIdent, content_hash_bytes};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_security::{
    AuthorizationReceipt, AuthorizationRequest, CatalogAction, CatalogConfigCapability,
    CredentialsVendCapability, GraphReadCapability, LineageReadCapability,
    NamespaceCreateCapability, NamespaceDropCapability, NamespaceListCapability,
    NamespaceLoadCapability, PolicyManageCapability, ProjectManageCapability, ReadRestriction,
    ServerManageCapability, StorageProfileManageCapability, TableCommitCapability,
    TableCreateCapability, TableDropCapability, TableLoadCapability, TableRestoreCapability,
    TableScanCapability, ViewDropCapability, ViewLoadCapability, ViewManageCapability,
    WarehouseManageCapability,
};
use serde_json::{Value, json};

use crate::*;

#[derive(Debug, Clone)]
pub(crate) struct RequestIdentity {
    pub(crate) principal: Principal,
    pub(crate) envelope: Value,
    pub(crate) typedid_envelope: Option<String>,
}

pub(crate) fn request_identity(headers: &HeaderMap) -> Result<RequestIdentity, LakeCatHttpError> {
    let header = |name: &str| -> Result<Option<&str>, LakeCatError> {
        let mut values = headers.get_all(name).iter();
        let Some(value) = values.next() else {
            return Ok(None);
        };
        if values.next().is_some() {
            return Err(LakeCatError::InvalidArgument(format!(
                "{name} header must appear at most once"
            )));
        }
        value
            .to_str()
            .map(Some)
            .map_err(|_| LakeCatError::InvalidArgument(format!("invalid UTF-8 in {name} header")))
    };

    let explicit_principal = header("x-lakecat-principal")?;
    let explicit_kind = header("x-lakecat-principal-kind")?
        .map(str::parse)
        .transpose()?;
    if explicit_kind.is_some() && explicit_principal.is_none() {
        return Err(LakeCatError::InvalidArgument(
            "x-lakecat-principal-kind requires x-lakecat-principal".to_string(),
        )
        .into());
    }
    if matches!(explicit_kind, Some(PrincipalKind::Anonymous)) {
        return Err(LakeCatError::InvalidArgument(
            "x-lakecat-principal-kind cannot be anonymous; omit identity headers for anonymous access"
                .to_string(),
        )
        .into());
    }
    let agent_did = header("x-lakecat-agent-did")?;
    let explicit_typedid = header("x-lakecat-typedid")?;
    let typedid = explicit_typedid.or(agent_did);
    let typedid_proof = header("x-lakecat-typedid-proof")?;
    let typedid_envelope = header("x-lakecat-typedid-envelope")?;
    if typedid_envelope.is_none() {
        if let Some(proof) = typedid_proof {
            return Err(LakeCatError::InvalidArgument(format!(
                "x-lakecat-typedid-proof requires x-lakecat-typedid-envelope; \
                 typedid-proof-hash={}",
                content_hash_bytes(proof.as_bytes())
            ))
            .into());
        }
    }
    let delegation = header("x-lakecat-agent-delegation")?;
    let signed_summary = header("x-lakecat-agent-summary-signature")?;
    let authorization = header("authorization")?;
    if authorization.is_some()
        && (explicit_principal.is_some() || agent_did.is_some() || explicit_typedid.is_some())
    {
        return Err(LakeCatError::InvalidArgument(
            "Authorization cannot be combined with x-lakecat-principal, x-lakecat-agent-did, or x-lakecat-typedid".to_string(),
        )
        .into());
    }

    let (principal, source, bearer_token_sha256) = if let Some(subject) = explicit_principal {
        (
            Principal::new(subject, explicit_kind.unwrap_or(PrincipalKind::Human))?,
            "x-lakecat-principal",
            None,
        )
    } else if let Some(did) = agent_did {
        (
            Principal::new(did, PrincipalKind::Agent)?,
            "x-lakecat-agent-did",
            None,
        )
    } else if let Some(did) = explicit_typedid {
        (
            Principal::new(did, PrincipalKind::Agent)?,
            "x-lakecat-typedid",
            None,
        )
    } else if let Some(authorization) = authorization {
        if let Some(token) = authorization.strip_prefix("Bearer ") {
            if token.trim().is_empty() {
                return Err(LakeCatError::InvalidArgument(
                    "Authorization Bearer token must not be empty".to_string(),
                )
                .into());
            }
            if token.bytes().any(|byte| byte.is_ascii_whitespace()) {
                return Err(LakeCatError::InvalidArgument(
                    "Authorization Bearer token must not contain whitespace".to_string(),
                )
                .into());
            }
            let token_sha256 = content_hash_bytes(token.as_bytes());
            (
                Principal::new(format!("bearer:{token_sha256}"), PrincipalKind::Service)?,
                "authorization",
                Some(token_sha256),
            )
        } else {
            return Err(LakeCatError::InvalidArgument(
                "unsupported Authorization scheme; use Bearer".to_string(),
            )
            .into());
        }
    } else {
        (Principal::anonymous(), "anonymous", None)
    };

    let agent_proof_allowed = principal.kind == PrincipalKind::Agent || typedid_envelope.is_some();
    if !agent_proof_allowed {
        if let Some(delegation) = delegation {
            return Err(LakeCatError::InvalidArgument(format!(
                "x-lakecat-agent-delegation requires an agent identity; \
                 agent-delegation-hash={}",
                content_hash_bytes(delegation.as_bytes())
            ))
            .into());
        }
        if let Some(signature) = signed_summary {
            return Err(LakeCatError::InvalidArgument(format!(
                "x-lakecat-agent-summary-signature requires an agent identity; \
                 agent-summary-signature-hash={}",
                content_hash_bytes(signature.as_bytes())
            ))
            .into());
        }
    }

    let envelope = json!({
        "type": "lakecat.request-identity.v1",
        "principal": principal,
        "source": source,
        "agent-did": agent_did,
        "typedid": typedid,
        "typedid-envelope-sha256": typedid_envelope
            .map(|value| content_hash_bytes(value.as_bytes())),
        "typedid-proof-sha256": typedid_proof.map(|value| content_hash_bytes(value.as_bytes())),
        "agent-delegation-sha256": delegation.map(|value| content_hash_bytes(value.as_bytes())),
        "agent-summary-signature-sha256": signed_summary
            .map(|value| content_hash_bytes(value.as_bytes())),
        "bearer-token-sha256": bearer_token_sha256,
        "attestation-state": "unverified",
        "raw-secret-material": false,
    });

    Ok(RequestIdentity {
        principal,
        envelope,
        typedid_envelope: typedid_envelope.map(ToString::to_string),
    })
}

pub(crate) fn request_idempotency_key(
    headers: &HeaderMap,
) -> Result<Option<String>, LakeCatHttpError> {
    let lakecat_key = single_idempotency_header(headers, "x-lakecat-idempotency-key")?;
    let standard_key = single_idempotency_header(headers, "idempotency-key")?;
    match (lakecat_key, standard_key) {
        (Some(lakecat_key), Some(standard_key)) => {
            if lakecat_key != standard_key {
                return Err(LakeCatError::InvalidArgument(
                    "Idempotency-Key and x-lakecat-idempotency-key must match when both are present"
                        .to_string(),
                )
                .into());
            }
            Ok(Some(lakecat_key))
        }
        (Some(key), None) | (None, Some(key)) => Ok(Some(key)),
        (None, None) => Ok(None),
    }
}

pub(crate) fn single_idempotency_header(
    headers: &HeaderMap,
    header_name: &str,
) -> Result<Option<String>, LakeCatHttpError> {
    let mut values = headers.get_all(header_name).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(LakeCatError::InvalidArgument(format!(
            "{header_name} must appear at most once"
        ))
        .into());
    }
    let key_bytes = value.as_bytes();
    if key_bytes.is_empty() || key_bytes.len() > 128 || !key_bytes.iter().all(u8::is_ascii) {
        return Err(LakeCatError::InvalidArgument(format!(
            "{header_name} must be 1..=128 ASCII characters"
        ))
        .into());
    }
    let key = std::str::from_utf8(key_bytes).map_err(|_| {
        LakeCatError::InvalidArgument(format!("{header_name} must be 1..=128 ASCII characters"))
    })?;
    if !key
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
    {
        return Err(LakeCatError::InvalidArgument(format!(
            "{header_name} may only contain A-Z, a-z, 0-9, '-', '_', '.', or ':'"
        ))
        .into());
    }
    Ok(Some(key.to_string()))
}

pub(crate) async fn verify_typedid_identity(
    state: &LakeCatState,
    mut identity: RequestIdentity,
) -> Result<RequestIdentity, LakeCatHttpError> {
    let Some(envelope_json) = identity.typedid_envelope.as_deref() else {
        return Ok(identity);
    };
    let verification = state
        .typedid_verifier
        .verify(envelope_json)
        .await
        .map_err(|err| redact_typedid_verifier_error(envelope_json, err))?;
    if identity.principal.kind != PrincipalKind::Anonymous
        && identity.principal != verification.principal
    {
        return Err(LakeCatError::Conflict(format!(
            "TypeDID verified subject does not match supplied principal; \
             verified-principal-hash={}; supplied-principal-hash={}",
            content_hash_bytes(verification.principal.subject.as_bytes()),
            content_hash_bytes(identity.principal.subject.as_bytes()),
        ))
        .into());
    }
    identity.principal = verification.principal.clone();
    identity.envelope["principal"] = json!(verification.principal);
    identity.envelope["source"] = json!("x-lakecat-typedid-envelope");
    identity.envelope["typedid"] = json!(identity.principal.subject);
    identity.envelope["attestation-state"] = json!("verified");
    identity.envelope["typedid-attestation"] = verification.attestation;
    Ok(identity)
}

pub(crate) fn redact_typedid_verifier_error(
    envelope_json: &str,
    err: LakeCatError,
) -> LakeCatError {
    let message = format!(
        "TypeDID envelope verification failed; typedid-envelope-hash={}; {}",
        content_hash_bytes(envelope_json.as_bytes()),
        error_detail_hash_context(&err),
    );
    match err {
        LakeCatError::InvalidArgument(_) => LakeCatError::InvalidArgument(message),
        LakeCatError::Conflict(_) => LakeCatError::Conflict(message),
        LakeCatError::NotSupported(_) => LakeCatError::NotSupported(message),
        LakeCatError::Internal(_) => LakeCatError::Internal(message),
        LakeCatError::NotFound { .. } => LakeCatError::NotFound {
            object: "TypeDID verifier failure",
            name: message,
        },
    }
}

pub(crate) async fn authorize(
    state: &LakeCatState,
    identity: RequestIdentity,
    action: CatalogAction,
    table: Option<TableIdent>,
) -> Result<AuthorizationReceipt, LakeCatHttpError> {
    let identity = verify_typedid_identity(state, identity).await?;
    let policy_bindings = if let Some(table) = table.as_ref() {
        state.store.policy_bindings_for_table(table).await?
    } else {
        Vec::new()
    };
    let read_restriction = if matches!(
        action,
        CatalogAction::TablePlanScan | CatalogAction::CredentialsVend
    ) && !policy_bindings.is_empty()
    {
        Some(read_restriction_from_policy_bindings(&policy_bindings)?)
    } else {
        None
    };
    let mut context = json!({
        "warehouse": state.warehouse.as_str(),
        "request-identity": identity.envelope,
        "policy-bindings": policy_bindings
            .iter()
            .map(policy_binding_response)
            .collect::<Vec<_>>(),
    });
    if let Some(restriction) = read_restriction {
        let raw_exception = matches!(action, CatalogAction::CredentialsVend)
            .then(|| raw_credential_exception_context(&restriction, &identity.principal));
        context["read-restriction"] = serde_json::to_value(restriction).map_err(|err| {
            LakeCatError::Internal(format!("failed to encode read restriction: {err}"))
        })?;
        if let Some(raw_exception) = raw_exception {
            context["lakecat:raw-credential-exception"] = raw_exception;
        }
    }
    let receipt = state
        .governance
        .authorize(AuthorizationRequest {
            principal: identity.principal,
            action,
            table,
            context,
        })
        .await?;
    if receipt.allowed {
        Ok(receipt.with_read_restriction_policy_hash()?)
    } else {
        Err(LakeCatError::Conflict("authorization denied".to_string()).into())
    }
}

pub(crate) fn raw_credential_exception_context(
    restriction: &ReadRestriction,
    principal: &Principal,
) -> Value {
    let governed_read_required = restriction.requires_governed_read();
    let trusted_human = principal.kind == PrincipalKind::Human;
    let allowed = !governed_read_required || trusted_human;
    json!({
        "requested": true,
        "allowed": allowed,
        "reason": if !governed_read_required {
            "restriction is compatible with short-lived credential vending"
        } else if trusted_human {
            "trusted human principal may use audited raw credential vending"
        } else {
            "fine-grained read restriction requires Sail-planned reads"
        },
    })
}

pub(crate) async fn authorize_table_create(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableCreateCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TableCreate,
        Some(table.clone()),
    )
    .await?;
    Ok(TableCreateCapability::from_receipt(receipt, table)?)
}

pub(crate) async fn authorize_catalog_config(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<CatalogConfigCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::CatalogConfig, None).await?;
    Ok(CatalogConfigCapability::from_receipt(receipt)?)
}

pub(crate) async fn authorize_namespace_create(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<NamespaceCreateCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::NamespaceCreate, None).await?;
    Ok(NamespaceCreateCapability::from_receipt(receipt)?)
}

pub(crate) async fn authorize_namespace_list(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<NamespaceListCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::NamespaceList, None).await?;
    Ok(NamespaceListCapability::from_receipt(receipt)?)
}

pub(crate) async fn authorize_namespace_load(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<NamespaceLoadCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::NamespaceLoad, None).await?;
    Ok(NamespaceLoadCapability::from_receipt(receipt)?)
}

pub(crate) async fn authorize_namespace_drop(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<NamespaceDropCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::NamespaceDrop, None).await?;
    Ok(NamespaceDropCapability::from_receipt(receipt)?)
}

pub(crate) async fn authorize_table_load(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableLoadCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TableLoad,
        Some(table.clone()),
    )
    .await?;
    Ok(TableLoadCapability::from_receipt(receipt, table)?)
}

pub(crate) async fn authorize_table_commit(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableCommitCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TableCommit,
        Some(table.clone()),
    )
    .await?;
    Ok(TableCommitCapability::from_receipt(receipt, table)?)
}

pub(crate) async fn authorize_table_drop(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableDropCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TableDrop,
        Some(table.clone()),
    )
    .await?;
    Ok(TableDropCapability::from_receipt(receipt, table)?)
}

pub(crate) async fn authorize_table_restore(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableRestoreCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TableRestore,
        Some(table.clone()),
    )
    .await?;
    Ok(TableRestoreCapability::from_receipt(receipt, table)?)
}

pub(crate) async fn authorize_table_scan(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableScanCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TablePlanScan,
        Some(table.clone()),
    )
    .await?;
    Ok(TableScanCapability::from_receipt(receipt, table)?)
}

pub(crate) async fn authorize_credentials_vend(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<CredentialsVendCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::CredentialsVend,
        Some(table.clone()),
    )
    .await?;
    Ok(CredentialsVendCapability::from_receipt(receipt, table)?)
}

pub(crate) async fn authorize_warehouse_manage(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<WarehouseManageCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::WarehouseManage, None).await?;
    Ok(WarehouseManageCapability::from_receipt(receipt)?)
}

pub(crate) async fn authorize_project_manage(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<ProjectManageCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::ProjectManage, None).await?;
    Ok(ProjectManageCapability::from_receipt(receipt)?)
}

pub(crate) async fn authorize_server_manage(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<ServerManageCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::ServerManage, None).await?;
    Ok(ServerManageCapability::from_receipt(receipt)?)
}

pub(crate) async fn authorize_storage_profile_manage(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<StorageProfileManageCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::StorageProfileManage, None).await?;
    Ok(StorageProfileManageCapability::from_receipt(receipt)?)
}

pub(crate) async fn authorize_view_manage(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<ViewManageCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::ViewManage, None).await?;
    Ok(ViewManageCapability::from_receipt(receipt)?)
}

pub(crate) async fn authorize_view_load(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<ViewLoadCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::ViewLoad, None).await?;
    Ok(ViewLoadCapability::from_receipt(receipt)?)
}

pub(crate) async fn authorize_view_drop(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<ViewDropCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::ViewDrop, None).await?;
    Ok(ViewDropCapability::from_receipt(receipt)?)
}

pub(crate) async fn authorize_policy_manage(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<PolicyManageCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::PolicyManage, None).await?;
    Ok(PolicyManageCapability::from_receipt(receipt)?)
}

pub(crate) async fn authorize_graph_read(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<GraphReadCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::GraphRead, None).await?;
    Ok(GraphReadCapability::from_receipt(receipt)?)
}

pub(crate) async fn authorize_lineage_read(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<LineageReadCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::LineageRead, None).await?;
    Ok(LineageReadCapability::from_receipt(receipt)?)
}
