//! src/routes/admin/logout.rs

use crate::error::Z2PResult;
use crate::session_state::TypedSession;
use crate::utils::see_other;
use actix_web::HttpResponse;
use actix_web_flash_messages::FlashMessage;

pub async fn log_out(session: TypedSession) -> Z2PResult<HttpResponse> {
    session.log_out();
    FlashMessage::info("You have successfully logged out.").send();
    Ok(see_other("/login"))
}
