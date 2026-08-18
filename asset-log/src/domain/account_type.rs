#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, serde::Serialize, serde::Deserialize)]
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
pub struct Account {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub account_type: AccountType,
    pub withholding: Option<bool>,   // Some(_) は tokutei のときだけ
    pub institution: Option<String>,
    pub currency: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AccountType {
    /// 非課税口座かどうか（損益計算・税引後リターンの分岐で使う）
    pub fn is_tax_exempt(self) -> bool {
        matches!(self, Self::NisaTsumitate | Self::NisaGrowth | Self::Ideco)
    }
}