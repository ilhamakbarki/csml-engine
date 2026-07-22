use actix_cors::Cors;
use actix_files as fs;
use actix_web::{http::header, middleware, web, App, HttpServer};
use csml_engine::make_migrations;
use csml_interpreter::csml_logs::init_logger;

mod routes;
mod apm;

const MAX_BODY_SIZE: usize = 8_388_608; // 8MB

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    let apm_enabled = match apm::init_apm() {
        Ok(enabled) => enabled,
        Err(err) => {
            eprintln!("⚠️ APM init failed: {}", err);
            false
        }
    };
    // init_logger() stays UNCONDITIONAL: csml_engine calls it internally from ~19 public
    // entry points (csml_engine/src/lib.rs:67, 193, 200, ... 461), so skipping it here
    // would change nothing except lose the module suppressions in csml_logs.rs:92-96 on
    // the first request. When tracing-log is absent this simply succeeds as before; when
    // present it fails harmlessly and apm.rs has already capped the bridge.
    init_logger();

    let server_port: String = match std::env::var("ENGINE_SERVER_PORT") {
        Ok(val) => val,
        Err(_) => "5000".to_owned(),
    };
    println!("CSML Server listening on port {}", server_port);

    // make migrations for PgSQL and do nothing for MongoDB and DynamoDB
    match make_migrations() {
        Ok(_) => (),
        Err(err) => panic!("PgSQL Migration ERROR: {:?}", err),
    };

    let server_result = HttpServer::new(|| {
        App::new()
            .wrap(
                Cors::default()
                    .send_wildcard()
                    .allowed_methods(vec!["GET", "POST", "DELETE"])
                    .allowed_headers(vec![
                        header::AUTHORIZATION,
                        header::ACCEPT,
                        header::CONTENT_TYPE,
                    ])
                    .max_age(86_400), //24h
            )
            .wrap(middleware::Logger::default())
            .app_data(web::JsonConfig::default().limit(MAX_BODY_SIZE))
            .service(fs::Files::new("/static", "./static").use_last_modified(true))
            .service(routes::index::home)
            .service(routes::validate::handler)
            .service(routes::status::get_status)
            .service(routes::run::handler)
            .service(routes::sns::handler)
            .service(routes::bot_versions::add_bot_version)
            .service(routes::bot_versions::get_bot_version)
            .service(routes::bot_versions::get_bot_latest_version)
            .service(routes::bot_versions::get_bot_latest_versions)
            .service(routes::bot_versions::delete_bot_version)
            .service(routes::bot_versions::delete_bot_versions)
            .service(routes::conversations::get_open)
            .service(routes::conversations::close_user_conversations)
            .service(routes::conversations::get_client_conversations)
            .service(routes::memories::create_client_memory)
            .service(routes::memories::get_memories)
            .service(routes::memories::get_memory)
            .service(routes::memories::delete_memories)
            .service(routes::memories::delete_memory)
            .service(routes::messages::get_client_messages)
            .service(routes::state::get_client_current_state)
            .service(routes::data::delete_expired_data)
            .service(routes::data::delete_bot)
            .service(routes::data::delete_client)
    })
    .bind(format!("0.0.0.0:{}", server_port))?
    .run()
    .await;

    if apm_enabled {
        // tracing-elastic-apm 3.4.0 has no flush()/shutdown(); give detached batches a
        // moment to reach the APM server before the runtime is torn down.
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    server_result
}
