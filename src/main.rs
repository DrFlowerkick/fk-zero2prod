//! main.rs

use zero2prod::run;
use std::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let listener = TcpListener::bind("localhost:8000")
        .expect("Failed to bind random port");
    run(listener)?.await
}
