// src/api/handlers/identity.rs
use actix_web::{web, HttpResponse};
use crate::core::identity::Template;

pub async fn register_identity(
    data: web::Json<IdentityRequest>,
    processor: web::Data<BiometricProcessor>,
) -> Result<HttpResponse> {
    let template = processor.process(&data.biometric_data).await?;
    let response = IdentityResponse {
        template_id: template.id.to_string(),
        proof: vec![],
        timestamp: chrono::Utc::now().timestamp(),
    };
    
    Ok(HttpResponse::Ok().json(response))
}

