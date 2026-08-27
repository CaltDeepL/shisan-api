use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use utoipa::ToSchema;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "asset_class", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    Equity,
    Etf,
    MutualFund,
    Bond,
    Cash,
    Other,
}

impl AssetClass {
    /// 市場価格を持つ区分かどうか。`cash` は常に額面評価のため価格登録の対象外。
    /// タスク#9の /holdings で評価方法を分岐する際にここを見る。
    pub fn is_priceable(self) -> bool {
        match self {
            Self::Equity | Self::Etf | Self::MutualFund | Self::Bond | Self::Other => true,
            Self::Cash => false,
        }
    }

    /// 基準価額が1万口あたりで公表される区分。
    /// `price_unit` の既定値を出し分けるためだけに使い、判定を handler に持ち出さない。
    pub fn default_price_unit(self) -> Decimal {
        match self {
            Self::MutualFund => Decimal::from(10_000),
            _ => Decimal::ONE,
        }
    }
}

#[derive(Debug, Clone, ToSchema)]
pub struct Asset {
    pub id: Uuid,
    pub user_id: Uuid,
    pub symbol: String,
    pub name: String,
    pub asset_class: AssetClass,
    pub currency: String,
    pub price_unit: Decimal,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AssetPrice {
    pub asset_id: Uuid,
    pub priced_on: NaiveDate,
    pub price: Decimal,
    pub source: String,
    pub updated_at: DateTime<Utc>,
}
