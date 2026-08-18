-- Add down migration script here
DROP TABLE IF EXISTS accounts;
DROP TABLE IF EXISTS users;
DROP TYPE  IF EXISTS account_type;
DROP FUNCTION IF EXISTS set_updated_at();