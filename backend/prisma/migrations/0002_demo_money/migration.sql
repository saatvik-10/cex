-- Demo money: mark accounts as demo and track the source of each balance so
-- real on-ramp funds can coexist with practice funds later.

ALTER TABLE "users" ADD COLUMN "is_demo" BOOLEAN NOT NULL DEFAULT true;

ALTER TABLE "balances" ADD COLUMN "source" VARCHAR(16) NOT NULL DEFAULT 'DEMO';

-- Relax the balance uniqueness key from (user_id, asset) to (user_id, asset,
-- source): the engine writes demo money today; a future REAL row (same user,
-- same asset) must not collide with it.
DROP INDEX "balances_user_id_asset_key";
CREATE UNIQUE INDEX "balances_user_id_asset_source_key" ON "balances"("user_id", "asset", "source");