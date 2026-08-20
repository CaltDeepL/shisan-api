-- 取引種別。入出金・配当は quantity/price の意味が変わるため
-- 別テーブル（cash_flows、タスク#11で追加予定）に分ける。
CREATE TYPE trade_kind AS ENUM ('buy', 'sell');

CREATE TABLE transactions (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    uuid NOT NULL REFERENCES users(id)    ON DELETE CASCADE,
    account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    asset_id   uuid NOT NULL REFERENCES assets(id)   ON DELETE RESTRICT,
    kind       trade_kind     NOT NULL,
    -- 数量は投信の口数を想定して小数8桁まで許容する
    quantity   numeric(20, 8) NOT NULL,
    -- 約定単価。price_unit あたりの呼値（assets.price_unit で割って金額になる）
    price      numeric(20, 8) NOT NULL,
    fee        numeric(20, 8) NOT NULL DEFAULT 0,
    traded_at  date           NOT NULL,
    note       text,
    created_at timestamptz    NOT NULL DEFAULT now(),

    CONSTRAINT transactions_quantity_positive  CHECK (quantity > 0),
    CONSTRAINT transactions_price_non_negative CHECK (price >= 0),
    CONSTRAINT transactions_fee_non_negative   CHECK (fee >= 0),
    CONSTRAINT transactions_note_not_blank     CHECK (note IS NULL OR btrim(note) <> '')
);

-- 総平均法の畳み込み用。build_holding に渡す順序をそのまま索引にする
CREATE INDEX transactions_position_idx
    ON transactions (account_id, asset_id, traded_at, created_at, id);

-- 一覧・期間フィルタ用
CREATE INDEX transactions_user_traded_idx
    ON transactions (user_id, traded_at DESC);