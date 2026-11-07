-- Backfill initial inventory movements for products created before the ledger.
INSERT INTO inventory_movements (product_id, change, reason, note)
SELECT id, stock_quantity, 'initial', 'backfill for pre-ledger product'
FROM products
WHERE stock_quantity > 0
  AND id NOT IN (SELECT product_id FROM inventory_movements);
