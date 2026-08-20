CREATE TABLE asset_prices (
    asset_id   uuid NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    priced_on  date NOT NULL,
    price      numeric(24,8) NOT NULL,
    source     text NOT NULL DEFAULT 'manual',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),

    PRIMARY KEY (asset_id, priced_on),

    CONSTRAINT asset_prices_price_non_negative CHECK (price >= 0),
    CONSTRAINT asset_prices_source_not_blank   CHECK (btrim(source) <> '')
);

CREATE TRIGGER asset_prices_set_updated_at
    BEFORE UPDATE ON asset_prices
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();
    