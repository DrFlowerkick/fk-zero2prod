//! main.rs

use std::net::TcpListener;
use zero2prod::run;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let listener = TcpListener::bind("localhost:8000").expect("Failed to bind random port");
    run(listener)?.await
}
