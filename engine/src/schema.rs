// @generated automatically by Diesel CLI or hand-maintained.
// Keep in sync with migrations/0001_init/up.sql and 0002_idempotency_keys/up.sql.

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
        source -> Varchar,
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

diesel::table! {
    idempotency_keys (user_id, key) {
        user_id -> Uuid,
        key -> Varchar,
        amount -> Numeric,
        created_at -> Timestamptz,
    }
}

diesel::joinable!(balances -> users (user_id));
diesel::joinable!(refresh_tokens -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(balances, users, refresh_tokens, idempotency_keys);
