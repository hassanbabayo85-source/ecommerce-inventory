-- Phase 1 / C1 — Order ownership (BOLA fix)
--
-- Every order now carries a per-order opaque access token. The token is
-- returned ONCE at order creation; clients must present it as the
-- `X-Order-Token` header on subsequent GET /api/orders/:id requests.
--
-- Existing orders are backfilled with fresh UUIDs — this is intentional:
-- there is no way to recover a token for orders created before this
-- migration, so they simply become inaccessible via the token-only path.
ALTER TABLE orders
    ADD COLUMN IF NOT EXISTS access_token UUID NOT NULL DEFAULT gen_random_uuid();

CREATE UNIQUE INDEX IF NOT EXISTS idx_orders_access_token
    ON orders (access_token);
