// @generated automatically by Diesel CLI or hand-maintained.
// Keep in sync with migrations/0001_init/up.sql.

diesel::table! {
    users (id) {
        id -> Uuid,
        username -> Varchar,
        password_hash -> Text,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    balances (id) {
        id -> Uuid,
        user_id -> Uuid,
        asset -> Varchar,
        amount -> Numeric,
    }
}

diesel::table! {
    refresh_tokens (id) {
        id -> Uuid,
        user_id -> Uuid,
        token_hash -> Text,
        expires_at -> Timestamptz,
        revoked -> Bool,
    }
}

diesel::joinable!(balances -> users (user_id));
diesel::joinable!(refresh_tokens -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(balances, users, refresh_tokens);
