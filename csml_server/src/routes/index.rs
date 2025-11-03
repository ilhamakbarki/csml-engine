use actix_web::{get, HttpResponse};
use tracing::{instrument};

#[get("/")]
#[instrument(name="GET /")]
pub async fn home() -> HttpResponse {
  HttpResponse::Ok()
    .content_type("text/html; charset=utf-8")
    .body(include_str!("../../static/index.html"))
}
