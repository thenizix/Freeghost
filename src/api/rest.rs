// src/api/rest.rs
use actix_web::{App, HttpServer, web};

pub struct RestApi {
    host: String,
    port: u16,
}

impl RestApi {
    pub async fn start(&self) -> Result<()> {
        HttpServer::new(|| {
            App::new()
                .service(web::resource("/identity/register")
                    .route(web::post().to(register_identity)))
        })
        .bind((self.host.as_str(), self.port))?
        .run()
        .await?;
        
        Ok(())
    }
}