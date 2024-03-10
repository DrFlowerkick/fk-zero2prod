//! src/startup.rs

use crate::routes::{health_check, subscribe};
use crate::email_client::EmailClient;
use actix_web::{dev::Server, web, web::Data, App, HttpServer};
use sqlx::PgPool;
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

pub fn run(listener: TcpListener, db_pool: PgPool, email_client: EmailClient) -> Result<Server, std::io::Error> {
    // Wrap the database pool and email client in a smart pointer
    let db_pool = Data::new(db_pool);
    let email_client = Data::new(email_client);
    // Capture `db_pool` from the surrounding environment
    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .route("/health_check", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
            .app_data(db_pool.clone())
            .app_data(email_client.clone())
    })
    .listen(listener)?
    .run();
    Ok(server)
}
