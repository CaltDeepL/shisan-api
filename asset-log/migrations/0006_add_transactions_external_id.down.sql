DROP INDEX IF EXISTS transactions_user_external_id_key;
ALTER TABLE transactions DROP COLUMN IF EXISTS external_id;
