use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::position::TradeKind;
use crate::error::AppError;
use crate::repository::transaction_repo::NewTransaction;
use crate::repository::{account_repo, asset_repo, snapshot_repo, transaction_repo};
use utoipa::ToSchema;

#[derive(Debug, serde::Deserialize)]
pub struct ImportRow {
    pub account: String,
    pub symbol: String,
    pub kind: TradeKind,
    pub quantity: Decimal,
    pub price: Decimal,
    pub fee: Decimal,
    pub traded_at: NaiveDate,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub note: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub external_id: Option<String>,
}

#[derive(Debug, serde::Serialize, ToSchema)]
pub struct ImportRowError {
    /// CSVの行番号（ヘッダ行を除く1始まり）
    pub row: usize,
    pub message: String,
}
/// 検証結果。dry-run のレスポンス、および本登録が失敗したときの422ボディ。
#[derive(Debug, serde::Serialize, ToSchema)]
pub struct ImportReport {
    pub total_rows: usize,
    /// 挿入対象になる行数
    pub to_insert: usize,
    /// 重複としてスキップされる行数
    pub to_skip_duplicate: usize,
    /// 1件でもあれば本登録は全体が失敗する
    pub errors: Vec<ImportRowError>,
}
/// 本登録が成功したときの結果。
#[derive(Debug, serde::Serialize, ToSchema)]
pub struct ImportResult {
    pub inserted: usize,
    pub skipped_duplicate: usize,
}

fn empty_string_as_none<'de, D>(de: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = serde::Deserialize::deserialize(de)?;
    let trimmed = s.trim();
    Ok(if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    })
}

struct ParsedRow {
    account_id: Uuid,
    asset_id: Uuid,
    price_unit: Decimal,
    kind: TradeKind,
    quantity: Decimal,
    price: Decimal,
    fee: Decimal,
    traded_at: NaiveDate,
    note: Option<String>,
    external_id: Option<String>,
    is_duplicate: bool,
}

impl ParsedRow {
    fn to_new_transaction(&self, user_id: Uuid) -> NewTransaction {
        NewTransaction {
            user_id,
            account_id: self.account_id,
            asset_id: self.asset_id,
            kind: self.kind,
            quantity: self.quantity,
            price: self.price,
            fee: self.fee,
            traded_at: self.traded_at,
            note: self.note.clone(),
            external_id: self.external_id.clone(),
        }
    }
}

struct ValidationOutcome {
    rows: Vec<ParsedRow>,
    errors: Vec<ImportRowError>,
    total_rows: usize,
}

impl ValidationOutcome {
    fn to_report(&self) -> ImportReport {
        let to_skip_duplicate = self.rows.iter().filter(|r| r.is_duplicate).count();
        ImportReport {
            total_rows: self.total_rows,
            to_insert: self.rows.len() - to_skip_duplicate,
            to_skip_duplicate,
            errors: self
                .errors
                .iter()
                .map(|e| ImportRowError {
                    row: e.row,
                    message: e.message.clone(),
                })
                .collect(),
        }
    }
}

async fn validate_csv(
    db: &PgPool,
    user_id: Uuid,
    csv_content: &str,
) -> Result<ValidationOutcome, AppError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(csv_content.as_bytes());

    let mut rows = Vec::new();
    let mut errors = Vec::new();
    let mut seen_external_ids: HashSet<String> = HashSet::new();
    let mut seen_natural_keys: HashSet<(Uuid, Uuid, String, String, String, NaiveDate)> =
        HashSet::new();
    let mut total_rows = 0usize;

    for (i, record) in reader.deserialize::<ImportRow>().enumerate() {
        let row_number = i + 1; // ヘッダーを除く1始まり
        total_rows += 1;

        let raw = match record {
            Ok(r) => r,
            Err(e) => {
                errors.push(ImportRowError {
                    row: row_number,
                    message: format!("CSV解析エラー: {e}"),
                });
                continue;
            }
        };

        let Some(account) = account_repo::find_by_name(db, user_id, &raw.account).await? else {
            errors.push(ImportRowError {
                row: row_number,
                message: format!("口座が見つかりません: {}", raw.account),
            });
            continue;
        };

        let Some(asset) = asset_repo::find_by_symbol(db, user_id, &raw.symbol).await? else {
            errors.push(ImportRowError {
                row: row_number,
                message: format!("銘柄が見つかりません: {}", raw.symbol),
            });
            continue;
        };

        if raw.quantity <= Decimal::ZERO {
            errors.push(ImportRowError {
                row: row_number,
                message: "数量は正の値である必要があります".to_owned(),
            });
            continue;
        }
        if raw.price < Decimal::ZERO || raw.fee < Decimal::ZERO {
            errors.push(ImportRowError {
                row: row_number,
                message: "価格・手数料は0以上である必要があります".to_owned(),
            });
            continue;
        }
        #[allow(clippy::collapsible_if)]
        if let Some(ext_id) = &raw.external_id {
            if seen_external_ids.contains(ext_id) {
                errors.push(ImportRowError {
                    row: row_number,
                    message: format!("CSV内でexternal_idが重複しています: {ext_id}"),
                });
                continue;
            }
        }

        let natural_key = (
            account.id,
            asset.id,
            format!("{:?}", raw.kind),
            raw.quantity.to_string(),
            raw.price.to_string(),
            raw.traded_at,
        );
        if raw.external_id.is_none() && seen_natural_keys.contains(&natural_key) {
            errors.push(ImportRowError {
                row: row_number,
                message: "CSV内で同一内容の取引が重複しています".to_owned(),
            });
            continue;
        }

        let is_duplicate = transaction_repo::find_duplicate(
            db,
            user_id,
            raw.external_id.as_deref(),
            account.id,
            asset.id,
            raw.kind,
            raw.quantity,
            raw.price,
            raw.traded_at,
        )
        .await?;

        if let Some(ext_id) = &raw.external_id {
            seen_external_ids.insert(ext_id.clone());
        }
        seen_natural_keys.insert(natural_key);

        rows.push(ParsedRow {
            account_id: account.id,
            asset_id: asset.id,
            price_unit: asset.price_unit,
            kind: raw.kind,
            quantity: raw.quantity,
            price: raw.price,
            fee: raw.fee,
            traded_at: raw.traded_at,
            note: raw.note,
            external_id: raw.external_id,
            is_duplicate,
        });
    }

    Ok(ValidationOutcome {
        rows,
        errors,
        total_rows,
    })
}

pub async fn dry_run_report(
    db: &PgPool,
    user_id: Uuid,
    csv_content: &str,
) -> Result<ImportReport, AppError> {
    Ok(validate_csv(db, user_id, csv_content).await?.to_report())
}

/// Ok(ImportResult) = 成功、Err(ImportReport) = 検証エラーで全体失敗(何も挿入していない)
pub async fn import(
    db: &PgPool,
    user_id: Uuid,
    csv_content: &str,
) -> Result<Result<ImportResult, ImportReport>, AppError> {
    let outcome = validate_csv(db, user_id, csv_content).await?;
    if !outcome.errors.is_empty() {
        return Ok(Err(outcome.to_report()));
    }

    // 影響を受ける(account_id, asset_id)をソートして収集(デッドロック回避のため、ロックは常に同じ順序で取得する)
    let mut touched: Vec<(Uuid, Uuid)> = outcome
        .rows
        .iter()
        .filter(|r| !r.is_duplicate)
        .map(|r| (r.account_id, r.asset_id))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    touched.sort();

    // 解決済みasset.price_unitをポジションごとに保持(fetch_position_contextは不要)
    let price_units: HashMap<(Uuid, Uuid), Decimal> = outcome
        .rows
        .iter()
        .map(|r| ((r.account_id, r.asset_id), r.price_unit))
        .collect();

    let mut tx = db.begin().await?;

    for (account_id, asset_id) in &touched {
        transaction_repo::lock_position(&mut tx, *account_id, *asset_id).await?;
    }

    let mut inserted = 0usize;
    for row in &outcome.rows {
        if row.is_duplicate {
            continue;
        }
        transaction_repo::insert(&mut tx, &row.to_new_transaction(user_id)).await?;
        inserted += 1;
    }

    for (account_id, asset_id) in &touched {
        let trades = transaction_repo::fetch_trades(&mut tx, *account_id, *asset_id).await?;
        let price_unit = price_units[&(*account_id, *asset_id)];
        if let Err(err) = crate::domain::position::build_holding(&trades, price_unit) {
            tx.rollback().await?;
            return Err(err.into());
        }
    }

    if let Some(earliest) = outcome.rows.iter().map(|row| row.traded_at).min() {
        snapshot_repo::invalidate_from(&mut tx, user_id, earliest).await?;
    }

    tx.commit().await?;
    let skipped_duplicate = outcome.rows.len() - inserted;
    Ok(Ok(ImportResult {
        inserted,
        skipped_duplicate,
    }))
}
