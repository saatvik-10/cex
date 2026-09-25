-- Initial CEX schema. Generated from the diesel-run tables (users, balances,
-- refresh_tokens) plus idempotency_keys, expressed as Prisma's 0001 migration.
-- On databases that already hold these tables (created by the legacy engine
-- runner), mark this migration applied with `prisma migrate resolve --applied`.

CREATE TABLE "users" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "username" VARCHAR(255) NOT NULL,
    "password_hash" TEXT NOT NULL,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "users_pkey" PRIMARY KEY ("id")
);

CREATE UNIQUE INDEX "users_username_key" ON "users"("username");

CREATE TABLE "refresh_tokens" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "user_id" UUID NOT NULL,
    "token_hash" TEXT NOT NULL,
    "expires_at" TIMESTAMPTZ(6) NOT NULL,
    "revoked" BOOLEAN NOT NULL DEFAULT false,
    CONSTRAINT "refresh_tokens_pkey" PRIMARY KEY ("id")
);

CREATE INDEX "refresh_tokens_user_id_idx" ON "refresh_tokens"("user_id");

CREATE TABLE "balances" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "user_id" UUID NOT NULL,
    "asset" VARCHAR(16) NOT NULL,
    "amount" DECIMAL(36, 18) NOT NULL DEFAULT 0,
    CONSTRAINT "balances_pkey" PRIMARY KEY ("id")
);

CREATE UNIQUE INDEX "balances_user_id_asset_key" ON "balances"("user_id", "asset");

CREATE INDEX "balances_user_id_idx" ON "balances"("user_id");

CREATE TABLE "idempotency_keys" (
    "user_id" UUID NOT NULL,
    "key" VARCHAR(255) NOT NULL,
    "amount" DECIMAL(36, 18) NOT NULL,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "idempotency_keys_pkey" PRIMARY KEY ("user_id", "key")
);

ALTER TABLE "refresh_tokens"
    ADD CONSTRAINT "refresh_tokens_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "users"("id") ON DELETE CASCADE ON UPDATE CASCADE;

ALTER TABLE "balances"
    ADD CONSTRAINT "balances_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "users"("id") ON DELETE CASCADE ON UPDATE CASCADE;