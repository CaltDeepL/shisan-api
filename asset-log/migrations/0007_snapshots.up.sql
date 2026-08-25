CREATE TABLE daily_snapshots (
    user_id          uuid          NOT NULL REFERENCES users(id)    ON DELETE CASCADE,
    snapshot_on      date          NOT NULL,
    account_id       uuid          NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    asset_id         uuid          NOT NULL REFERENCES assets(id)   ON DELETE CASCADE,
    quantity         numeric(20,8) NOT NULL,
    avg_cost         numeric(20,8) NOT NULL,
    cost_basis_jpy   numeric(20,2) NOT NULL,
    market_value_jpy numeric(20,2),
    price            numeric(20,8),
    unpriced         boolean       NOT NULL DEFAULT false,
    PRIMARY KEY (user_id, snapshot_on, account_id, asset_id),
    CONSTRAINT daily_snapshots_quantity_positive CHECK (quantity > 0)
);

COMMENT ON TABLE daily_snapshots IS
    '/analytics/asset-history の評価結果キャッシュ。正本は取引履歴からの再計算';
COMMENT ON COLUMN daily_snapshots.avg_cost IS
    '約定日レートで換算したJPY建ての平均取得単価。表示用で、集計には使わない';
COMMENT ON COLUMN daily_snapshots.cost_basis_jpy IS
    '約定日レート換算の累積簿価。日次の為替レートからは復元できない';
COMMENT ON COLUMN daily_snapshots.price IS
    'その日の評価に使った価格（資産通貨建て・price_unit 未除算）。監査用';
COMMENT ON COLUMN daily_snapshots.unpriced IS
    '価格または為替レートが引けず評価から外したポジション。market_value_jpy は NULL';

CREATE INDEX daily_snapshots_user_date_idx ON daily_snapshots (user_id, snapshot_on);

-- その日を計算済みであることを表すマーカー。保有ゼロの日も行が入る。
-- 「daily_snapshots に行が無い」だけでは未計算と保有ゼロが区別できないため必要。
CREATE TABLE snapshot_days (
    user_id        uuid        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    snapshot_on    date        NOT NULL,
    position_count integer     NOT NULL,
    unpriced_count integer     NOT NULL,
    computed_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, snapshot_on)
);