use actix_web::{
    web::{self, Data, Json, Path},
    HttpResponse, Scope,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tracing::{info, warn, error};

use crate::{
    core::{
        identity::types::{Identity, DeviceInfo, BehaviorPattern},
        services::identity::IdentityService,
        crypto::quantum::ZeroKnowledgeProof,
    },
    utils::error::NodeError,
};

#[derive(Debug, Deserialize)]
pub struct CreateIdentityRequest {
    pub biometric_data: Vec<u8>,
    pub device_info: Option<DeviceInfo>,
}

#[derive(Debug, Deserialize)]
pub struct VerifyIdentityRequest {
    pub biometric_data: Vec<u8>,
    pub proof: ZeroKnowledgeProof,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBehaviorRequest {
    pub pattern: BehaviorPattern,
}

#[derive(Debug, Serialize)]
pub struct IdentityResponse {
    pub id: Uuid,
    pub verification_status: String,
    pub trust_score: f32,
    pub risk_score: f32,
}

impl From<&Identity> for IdentityResponse {
    fn from(identity: &Identity) -> Self {
        Self {
            id: identity.id,
            verification_status: format!("{:?}", identity.verification_status),
            trust_score: identity.behavior_profile.trust_score,
            risk_score: identity.metadata.risk_score,
        }
    }
}

pub fn scope() -> Scope {
    web::scope("/identity")
        .service(
            web::resource("")
                .route(web::post().to(create_identity))
        )
        .service(
            web::resource("/{id}")
                .route(web::get().to(get_identity))
                .route(web::delete().to(revoke_identity))
        )
        .service(
            web::resource("/{id}/verify")
                .route(web::post().to(verify_identity))
        )
        .service(
            web::resource("/{id}/behavior")
                .route(web::post().to(update_behavior))
        )
}

async fn create_identity(
    service: Data<IdentityService>,
    request: Json<CreateIdentityRequest>,
) -> Result<HttpResponse, actix_web::Error> {
    info!("Received identity creation request");

    let identity = service
        .create_identity(request.biometric_data.clone(), request.device_info.clone())
        .await
        .map_err(|e| {
            error!("Identity creation failed: {}", e);
            actix_web::error::ErrorInternalServerError(e)
        })?;

    info!("Created identity: {}", identity.id);
    Ok(HttpResponse::Created().json(IdentityResponse::from(&identity)))
}

async fn get_identity(
    service: Data<IdentityService>,
    id: Path<Uuid>,
) -> Result<HttpResponse, actix_web::Error> {
    let identity = service
        .get_identity(&id)
        .await
        .map_err(|e| {
            error!("Failed to retrieve identity {}: {}", id, e);
            actix_web::error::ErrorInternalServerError(e)
        })?
        .ok_or_else(|| {
            warn!("Identity {} not found", id);
            actix_web::error::ErrorNotFound(NodeError::Identity("Identity not found".into()))
        })?;

    Ok(HttpResponse::Ok().json(IdentityResponse::from(&identity)))
}

async fn verify_identity(
    service: Data<IdentityService>,
    id: Path<Uuid>,
    request: Json<VerifyIdentityRequest>,
) -> Result<HttpResponse, actix_web::Error> {
    info!("Received verification request for identity: {}", id);

    let verified = service
        .verify_identity(
            *id,
            request.biometric_data.clone(),
            request.proof.clone(),
        )
        .await
        .map_err(|e| {
            error!("Verification failed for identity {}: {}", id, e);
            actix_web::error::ErrorInternalServerError(e)
        })?;

    info!("Identity {} verification result: {}", id, verified);
    Ok(HttpResponse::Ok().json(json!({ "verified": verified })))
}

async fn update_behavior(
    service: Data<IdentityService>,
    id: Path<Uuid>,
    request: Json<UpdateBehaviorRequest>,
) -> Result<HttpResponse, actix_web::Error> {
    info!("Updating behavior for identity: {}", id);

    service
        .update_behavior(*id, request.pattern.clone())
        .await
        .map_err(|e| {
            error!("Behavior update failed for identity {}: {}", id, e);
            actix_web::error::ErrorInternalServerError(e)
        })?;

    Ok(HttpResponse::Ok().finish())
}

async fn revoke_identity(
    service: Data<IdentityService>,
    id: Path<Uuid>,
) -> Result<HttpResponse, actix_web::Error> {
    info!("Revoking identity: {}", id);

    service
        .revoke_identity(*id)
        .await
        .map_err(|e| {
            error!("Failed to revoke identity {}: {}", id, e);
            actix_web::error::ErrorInternalServerError(e)
        })?;

    info!("Identity {} revoked successfully", id);
    Ok(HttpResponse::Ok().finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use crate::core::identity::types::BiometricTemplate;

    #[actix_web::test]
    async fn test_create_identity() {
        // TODO: Implement tests
    }

    #[actix_web::test]
    async fn test_verify_identity() {
        // TODO: Implement tests
    }

    #[actix_web::test]
    async fn test_update_behavior() {
        // TODO: Implement tests
    }

    #[actix_web::test]
    async fn test_revoke_identity() {
        // TODO: Implement tests
    }
}
