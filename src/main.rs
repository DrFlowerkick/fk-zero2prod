//! main.rs

use std::fmt::{Debug, Display};
use tokio::task::JoinError;
use zero2prod::configuration::get_configuration;
use zero2prod::error::Z2PResult;
use zero2prod::idempotency::run_cleanup_worker_until_stopped;
use zero2prod::issue_delivery_worker::run_delivery_worker_until_stopped;
use zero2prod::startup::Application;
use zero2prod::telemetry::{get_subscriber, init_subscriber};

#[tokio::main]
async fn main() -> Z2PResult<()> {
    let subscriber = get_subscriber("zero2prod".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);

    // Panic if we can't read configuration
    let configuration = get_configuration().expect("Failed to read configuration.");
    let application = Application::build(configuration.clone()).await?;
    let application_task = tokio::spawn(application.run_until_stopped());
    let delivery_worker_task =
        tokio::spawn(run_delivery_worker_until_stopped(configuration.clone()));
    let cleanup_idempotency_keys = tokio::spawn(run_cleanup_worker_until_stopped(configuration));

    tokio::select! {
        o = application_task => report_exit("API", o),
        o = delivery_worker_task => report_exit("Background delivery worker", o),
        o = cleanup_idempotency_keys => report_exit("Background cleanup of idempotency keys", o),
    };

    Ok(())
}

fn report_exit(task_name: &str, outcome: Result<Result<(), impl Debug + Display>, JoinError>) {
    match outcome {
        Ok(Ok(())) => {
            tracing::info!("{} has exited", task_name)
        }
        Ok(Err(e)) => {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "{} failed",
                task_name
            )
        }
        Err(e) => {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "{} task failed to complete",
                task_name
            )
        }
    }
}
