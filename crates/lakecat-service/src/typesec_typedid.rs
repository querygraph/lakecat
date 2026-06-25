use std::sync::Arc;

use async_trait::async_trait;
use lakecat_core::{LakeCatError, LakeCatResult, Principal, PrincipalKind, content_hash_bytes};
use typesec::{DidEnvelope, TypeDidGateway};

use crate::{TypeDidVerification, TypeDidVerifier, error_detail_hash_context};

pub struct TypeSecTypeDidVerifier {
    gateway: Arc<TypeDidGateway>,
}

impl TypeSecTypeDidVerifier {
    pub fn new(gateway: Arc<TypeDidGateway>) -> Arc<Self> {
        Arc::new(Self { gateway })
    }
}

#[async_trait]
impl TypeDidVerifier for TypeSecTypeDidVerifier {
    async fn verify(&self, envelope_json: &str) -> LakeCatResult<TypeDidVerification> {
        let envelope: DidEnvelope = serde_json::from_str(envelope_json).map_err(|err| {
            LakeCatError::InvalidArgument(format!(
                "invalid TypeDID envelope JSON; typedid-envelope-hash={}; {}",
                content_hash_bytes(envelope_json.as_bytes()),
                error_detail_hash_context(err),
            ))
        })?;
        let verified = self.gateway.open_message(&envelope).map_err(|err| {
            LakeCatError::Conflict(format!(
                "TypeSec rejected TypeDID envelope; typedid-envelope-hash={}; {}",
                content_hash_bytes(envelope_json.as_bytes()),
                error_detail_hash_context(err),
            ))
        })?;
        let attestation = verified.attestation();
        Ok(TypeDidVerification {
            principal: Principal::new(attestation.subject.to_string(), PrincipalKind::Agent)?,
            attestation: serde_json::to_value(attestation).map_err(|err| {
                LakeCatError::Internal(format!("failed to encode TypeDID attestation: {err}"))
            })?,
        })
    }
}
