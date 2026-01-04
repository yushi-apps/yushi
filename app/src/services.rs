use actix_web::{post, HttpResponse, Responder};
use uuid::Uuid;

#[post("/chat")]
async fn chat(req_body: String) -> impl Responder {
    let id = Uuid::new_v4().to_string();
    match jieyusha::chat(&req_body, &id).await {
        Ok(response) => HttpResponse::Ok().json(response),
        Err(err) => HttpResponse::InternalServerError().body(err.to_string()),
    }
}