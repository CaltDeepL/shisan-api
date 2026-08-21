CREATE TABLE fx_rates (
    base       character(3)   NOT NULL,
    quote      character(3)   NOT NULL,
    rated_on   date           NOT NULL,
    rate       numeric(20,10) NOT NULL,
    fetched_at timestamptz    NOT NULL DEFAULT now(),
    PRIMARY KEY (base, quote, rated_on),
    CONSTRAINT fx_rates_base_format   CHECK (base  ~ '^[A-Z]{3}$'),
    CONSTRAINT fx_rates_quote_format  CHECK (quote ~ '^[A-Z]{3}$'),
    CONSTRAINT fx_rates_differ        CHECK (base <> quote),
    CONSTRAINT fx_rates_rate_positive CHECK (rate > 0)
);

COMMENT ON COLUMN fx_rates.rated_on IS 'ECBが公表した日。要求した日付ではない';
COMMENT ON COLUMN fx_rates.fetched_at IS '外部APIから取得した時刻。フォールバック時の説明に使う';