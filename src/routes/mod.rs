use actix_web::web;

use crate::handlers::{user, wallet};

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/auth")
            .service(user::sign_up)
            .service(user::sign_in)
            .service(user::refresh_tokens)
            .service(user::profile),
    )
    .service(
        web::scope("/wallet")
            .service(wallet::get_balance)
            .service(wallet::onramp),
    );
}
