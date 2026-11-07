-- Hana UPDATE ko DELETE a kan inventory_movements don tabbatar da audit trail.
CREATE OR REPLACE FUNCTION prevent_ledger_modification()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'inventory_movements is append-only. UPDATE and DELETE are forbidden.';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_prevent_ledger_update ON inventory_movements;
CREATE TRIGGER trg_prevent_ledger_update
    BEFORE UPDATE OR DELETE ON inventory_movements
    FOR EACH ROW EXECUTE FUNCTION prevent_ledger_modification();
