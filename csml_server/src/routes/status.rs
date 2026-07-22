use actix_web::{get, HttpResponse};
use std::thread;
use tracing::Span;

/*
* Get Server status
*
* {"statusCode": 200}
*
*/
#[get("/status")]
#[tracing::instrument(name="GET /status", skip_all)]
pub async fn get_status() -> HttpResponse {
    let span = Span::current();
    let res = thread::spawn(move || {
        let _guard = span.entered();
        csml_engine::get_status()
    }).join().unwrap();

    match res {
        Ok(data) => HttpResponse::Ok().json(data),
        Err(err) => {
            eprintln!("EngineError: {:?}", err);
            tracing::error!("EngineError: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}