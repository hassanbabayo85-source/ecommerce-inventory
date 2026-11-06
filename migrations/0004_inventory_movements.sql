-- Phase 3 — Inventory Movement Ledger
-- Append-only audit trail of every stock change.
--
-- INVARIANT: for any product, SUM(change) == current stock_quantity.
-- Verified by tests/phase3_inventory.rs::ledger_reconciles_with_current_stock.
--
-- ON DELETE RESTRICT on product_id ensures we never lose audit history
-- as a side effect of deleting a product.

CREATE TABLE IF NOT EXISTS inventory_movements (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id    UUID NOT NULL REFERENCES products(id) ON DELETE RESTRICT,
    change        INTEGER NOT NULL CHECK (change <> 0),
    reason        TEXT NOT NULL CHECK (reason IN (
        'initial',
        'order_reservation',
        'order_cancellation',
        'admin_adjustment',
        'stock_sync'
    )),
    reference_id  UUID NULL,
    note          TEXT NULL CHECK (note IS NULL OR char_length(note) <= 1000),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_inv_movements_product_created
    ON inventory_movements (product_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_inv_movements_reference
    ON inventory_movements (reference_id) WHERE reference_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_inv_movements_reason
    ON inventory_movements (reason);
