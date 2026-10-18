ALTER TABLE transactions
    ADD COLUMN external_id text;

CREATE UNIQUE INDEX transactions_user_external_id_key
    ON transactions (user_id, external_id)
    WHERE external_id IS NOT NULL;