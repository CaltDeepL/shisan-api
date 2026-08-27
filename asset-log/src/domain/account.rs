use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "account_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AccountType {
    Tokutei,
    Ippan,
    NisaTsumitate,
    NisaGrowth,
    Ideco,
    Bank,
}

impl AccountType {
    /// 非課税口座かどうか。タスク#7の domain::position で課税判定に使う。
    /// matches! ではなく match で全バリアントを列挙しているのは、
    /// ENUM に値を足したときにここでコンパイルエラーを出すため。
    pub fn is_tax_exempt(self) -> bool {
        match self {
            Self::NisaTsumitate | Self::NisaGrowth | Self::Ideco => true,
            Self::Tokutei | Self::Ippan | Self::Bank => false,
        }
    }

    /// DB の accounts_withholding_only_tokutei と対になる判定
    pub fn requires_withholding(self) -> bool {
        matches!(self, Self::Tokutei)
    }

    /// 有価証券を保有しうる口座か（bank は残高のみ）
    pub fn holds_securities(self) -> bool {
        !matches!(self, Self::Bank)
    }
}

/// 内部モデル。API レスポンスは handler 側の DTO に変換して返す
#[derive(Debug, Clone, ToSchema)]
pub struct Account {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub account_type: AccountType,
    pub withholding: Option<bool>,
    pub institution: Option<String>,
    pub currency: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 口座の新規作成に必要な値
#[derive(Debug)]
pub struct NewAccount<'a> {
    pub name: &'a str,
    pub account_type: AccountType,
    pub withholding: Option<bool>,
    pub institution: Option<&'a str>,
    pub currency: &'a str,
}

/// 口座の部分更新。外側 None = 変更しない、Some(None) = NULL にする
#[derive(Debug, Default)]
pub struct AccountPatch<'a> {
    pub name: Option<&'a str>,
    pub institution: Option<Option<&'a str>>,
    pub withholding: Option<Option<bool>>,
}

impl AccountPatch<'_> {
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.institution.is_none() && self.withholding.is_none()
    }
}
