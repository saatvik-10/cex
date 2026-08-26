use serde::{Deserialize, Serialize};

pub struct User {
    pub id: u32,
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize)]
pub struct SignInInput {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize)]
pub struct SignUpInput {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize)]
pub struct SignInRes {
    pub msg: String,
}

#[derive(Serialize, Deserialize)]
pub struct SignUpRes {
    pub msg: String,
}
