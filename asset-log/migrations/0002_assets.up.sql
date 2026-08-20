-- Add up migration script here
CREATE TYPE asset_class AS ENUM (
    'equity',
    'etf',
    'mutual_fund',
    'bond',
    'cash',
    'other'
);

CREATE TABLE assets (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    symbol      text NOT NULL,
    name        text NOT NULL,
    asset_class asset_class NOT NULL,
    currency    character(3) NOT NULL DEFAULT 'JPY',
    -- 投信の基準価額は1万口あたりのため単位を保持する。
    -- domain::position の evaluate() が 現在価格 ÷ price_unit で使う。
    price_unit  numeric(12,0) NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT assets_symbol_not_blank   CHECK (btrim(symbol) <> ''),
    CONSTRAINT assets_name_not_blank     CHECK (btrim(name) <> ''),
    CONSTRAINT assets_currency_format    CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT assets_price_unit_positive CHECK (price_unit > 0)
);

-- symbol は大文字小文字を同一視する（voo と VOO を別銘柄にしない）。
-- users_email_lower_key と同じ関数インデックス方式。
CREATE UNIQUE INDEX assets_user_symbol_key ON assets (user_id, upper(symbol));

CREATE INDEX assets_user_id_idx ON assets (user_id);

CREATE TRIGGER assets_set_updated_at
    BEFORE UPDATE ON assets
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();