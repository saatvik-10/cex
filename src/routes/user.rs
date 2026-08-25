use actix_web::{HttpResponse, Responder, get, post};

#[post("/signup")]
async fn sign_up() -> impl Responder {}

#[post("/signin")]
async fn sign_in() -> impl Response {}
