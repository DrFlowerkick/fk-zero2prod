//! src/startup.rs

use crate::routes::{health_check, subscribe};
use actix_web::{dev::Server, middleware::Logger, web, web::Data, App, HttpServer};
use sqlx::PgPool;
use std::net::TcpListener;

pub fn run(listener: TcpListener, db_pool: PgPool) -> Result<Server, std::io::Error> {
    // Wrap the database pool in a smart pointer
    let db_pool = Data::new(db_pool);
    // Capture `db_pool` from the surrounding environment
    let server = HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .route("/health_check", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
            .app_data(db_pool.clone())
    })
    .listen(listener)?
    .run();
    Ok(server)
}
