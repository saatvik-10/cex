use std::sync::Mutex;

use crate::{
    routes::user::{sign_in, sign_up},
    types::user::User,
};
use actix_web::{App, HttpServer, web};

pub mod routes;
pub mod types;

struct AppState {
    users: Mutex<Vec<User>>,
    user_index: Mutex<u32>,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let app_state = web::Data::new(AppState {
        users: Mutex::new(vec![]),
        user_index: Mutex::new(0),
    });

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .service(sign_in)
            .service(sign_up)
    })
    .bind(("127.0.0.1", 8000))?
    .run()
    .await
}
