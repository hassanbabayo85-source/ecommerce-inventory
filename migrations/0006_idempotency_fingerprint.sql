-- Add fingerprint columns to prevent idempotency key reuse with a different payload.
ALTER TABLE idempotency_keys
    ADD COLUMN customer_email TEXT NOT NULL DEFAULT '',
    ADD COLUMN request_fingerprint TEXT NOT NULL DEFAULT '';
