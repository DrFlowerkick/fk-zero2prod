//! src/routes/login/post.rs

use crate::authentication::{validate_credentials, Credentials};
use crate::error::Error;
use crate::session_state::TypedSession;
use crate::utils::see_other;
use actix_web::{error::InternalError, web, HttpResponse};
use actix_web_flash_messages::FlashMessage;
use secrecy::Secret;
use sqlx::PgPool;

#[derive(serde::Deserialize)]
pub struct FormData {
    username: String,
    password: Secret<String>,
}

#[tracing::instrument(
    skip(form, pool, session),
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
pub async fn login(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
    session: TypedSession,
) -> Result<HttpResponse, InternalError<Error>> {
    let credentials = Credentials {
        username: form.0.username,
        password: form.0.password,
    };
    tracing::Span::current().record("username", &tracing::field::display(&credentials.username));
    match validate_credentials(credentials, &pool).await {
        Ok(user_id) => {
            tracing::Span::current().record("user_id", &tracing::field::display(&user_id));
            session.renew();
            session
                .insert_user_id(user_id)
                .map_err(|e| login_redirect(Error::UnexpectedError(e.into())))?;
            Ok(see_other("/admin/dashboard"))
        }
        Err(e) => Err(login_redirect(Error::auth_error_to_login_error(e))),
    }
}

fn login_redirect(e: Error) -> InternalError<Error> {
    FlashMessage::error(e.to_string()).send();
    let response = see_other("/login");
    InternalError::from_response(e, response)
}
