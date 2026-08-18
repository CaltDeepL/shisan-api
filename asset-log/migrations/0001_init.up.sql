-- Add up migration script here
-- 更新時刻を自動更新するための共通関数
CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- 口座種別
CREATE TYPE account_type AS ENUM (
    'tokutei',
    'ippan',
    'nisa_tsumitate',
    'nisa_growth',
    'ideco',
    'bank'
);

CREATE TABLE users (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    email         TEXT        NOT NULL,
    password_hash TEXT        NOT NULL,
    display_name  TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 大文字小文字を区別せずメールの一意性を担保
CREATE UNIQUE INDEX users_email_lower_key ON users (lower(email));

CREATE TRIGGER users_set_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE accounts (
    id           UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID         NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         TEXT         NOT NULL,
    account_type account_type NOT NULL,
    withholding  BOOLEAN,  
    institution  TEXT,                                   -- 例: SBI証券
    currency     CHAR(3)      NOT NULL DEFAULT 'JPY',
    created_at   TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ  NOT NULL DEFAULT now(),
    CONSTRAINT accounts_currency_format CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT accounts_name_not_blank  CHECK (length(btrim(name)) > 0),
    CONSTRAINT accounts_user_name_key   UNIQUE (user_id, name),
    -- 特定口座なら必須、それ以外ならNULLでなければならない
    CONSTRAINT accounts_withholding_only_tokutei CHECK (
        (account_type = 'tokutei' AND withholding IS NOT NULL)
        OR
        (account_type <> 'tokutei' AND withholding IS NULL)
    )
);

-- 「自分の口座一覧」が最頻クエリになるため
CREATE INDEX accounts_user_id_idx ON accounts (user_id);

CREATE TRIGGER accounts_set_updated_at
    BEFORE UPDATE ON accounts
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();