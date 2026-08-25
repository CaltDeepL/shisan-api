use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::account::AccountType;
use crate::domain::asset::AssetClass;

/// 1ポジション×1日分の保存行。
pub struct SnapshotRow {
    pub account_id: Uuid,
    pub asset_id: Uuid,
    pub quantity: Decimal,
    pub avg_cost: Decimal,
    pub cost_basis_jpy: Decimal,
    pub market_value_jpy: Option<Decimal>,
    pub price: Option<Decimal>,
    pub unpriced: bool,
}

/// 読み出し時の行。group_by 用に口座名・銘柄名・口座種別・資産クラスを JOIN で載せる。
/// asset_history のキャッシュ経路が group_of() に渡せるよう HistoryTrade 相当の項目を持つ。
pub struct SnapshotWithMeta {
    pub snapshot_on: NaiveDate,
    pub account_id: Uuid,
    pub asset_id: Uuid,
    pub account_name: String,
    pub account_type: AccountType,
    pub symbol: String,
    pub asset_name: String,
    pub asset_class: AssetClass,
    pub quantity: Decimal,
    pub cost_basis_jpy: Decimal,
    pub market_value_jpy: Option<Decimal>,
}

pub async fn find_in_range(
    conn: &mut PgConnection,
    user_id: Uuid,
    days: &[NaiveDate],
) -> Result<Vec<SnapshotWithMeta>, sqlx::Error> {
    sqlx::query_as!(
        SnapshotWithMeta,
        r#"
        SELECT
            s.snapshot_on,
            s.account_id,
            s.asset_id,
            a.name           AS account_name,
            a.account_type   AS "account_type: AccountType",
            ast.symbol,
            ast.name         AS asset_name,
            ast.asset_class  AS "asset_class: AssetClass",
            s.quantity,
            s.cost_basis_jpy,
            s.market_value_jpy
        FROM daily_snapshots s
        JOIN accounts a   ON a.id   = s.account_id
        JOIN assets   ast ON ast.id = s.asset_id
        WHERE s.user_id = $1 AND s.snapshot_on = ANY($2::date[])
        ORDER BY s.snapshot_on, s.account_id, s.asset_id
        "#,
        user_id,
        days,
    )
    .fetch_all(conn)
    .await
}

/// 指定した日のうち、スナップショット計算済みの日数を返す。
/// `daily_snapshots` ではなく `snapshot_days` で見るのは、
/// 保有ゼロの日を「未計算」と誤判定しないため。
pub async fn covered_days(
    conn: &mut PgConnection,
    user_id: Uuid,
    days: &[NaiveDate],
) -> Result<i64, sqlx::Error> {
    if days.is_empty() {
        return Ok(0);
    }

    sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
        FROM snapshot_days
        WHERE user_id = $1 AND snapshot_on = ANY($2::date[])
        "#,
        user_id,
        days,
    )
    .fetch_one(conn)
    .await
}

/// 1ユーザー・1日分を保存する。保有ゼロの日でも snapshot_days には行を入れる。
pub async fn upsert_day(
    conn: &mut PgConnection,
    user_id: Uuid,
    on: NaiveDate,
    rows: &[SnapshotRow],
) -> Result<(), sqlx::Error> {
    // 前回計算時に存在したが今回消えたポジションを残さないため、先に当日分を消す
    sqlx::query!(
        "DELETE FROM daily_snapshots WHERE user_id = $1 AND snapshot_on = $2",
        user_id,
        on,
    )
    .execute(&mut *conn)
    .await?;

    if !rows.is_empty() {
        let account_ids: Vec<Uuid> = rows.iter().map(|r| r.account_id).collect();
        let asset_ids: Vec<Uuid> = rows.iter().map(|r| r.asset_id).collect();
        let quantities: Vec<Decimal> = rows.iter().map(|r| r.quantity).collect();
        let avg_costs: Vec<Decimal> = rows.iter().map(|r| r.avg_cost).collect();
        let cost_jpy: Vec<Decimal> = rows.iter().map(|r| r.cost_basis_jpy).collect();
        let mv_jpy: Vec<Option<Decimal>> = rows.iter().map(|r| r.market_value_jpy).collect();
        let prices: Vec<Option<Decimal>> = rows.iter().map(|r| r.price).collect();
        let unpriced_flags: Vec<bool> = rows.iter().map(|r| r.unpriced).collect();

        sqlx::query!(
            r#"
            INSERT INTO daily_snapshots (
                user_id, snapshot_on, account_id, asset_id,
                quantity, avg_cost, cost_basis_jpy, market_value_jpy, price, unpriced
            )
            SELECT $1::uuid, $2::date, t.account_id, t.asset_id,
                   t.quantity, t.avg_cost, t.cost_basis_jpy,
                   t.market_value_jpy, t.price, t.unpriced
            FROM unnest(
                $3::uuid[], $4::uuid[], $5::numeric[], $6::numeric[],
                $7::numeric[], $8::numeric[], $9::numeric[], $10::bool[]
            ) AS t(
                account_id, asset_id, quantity, avg_cost,
                cost_basis_jpy, market_value_jpy, price, unpriced
            )
            "#,
            user_id,
            on,
            &account_ids,
            &asset_ids,
            &quantities,
            &avg_costs,
            &cost_jpy,
            &mv_jpy as &[Option<Decimal>],
            &prices as &[Option<Decimal>],
            &unpriced_flags,
        )
        .execute(&mut *conn)
        .await?;
    }

    let unpriced = rows.iter().filter(|r| r.unpriced).count() as i32;

    sqlx::query!(
        r#"
        INSERT INTO snapshot_days (user_id, snapshot_on, position_count, unpriced_count)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (user_id, snapshot_on)
        DO UPDATE SET position_count = EXCLUDED.position_count,
                      unpriced_count = EXCLUDED.unpriced_count,
                      computed_at    = now()
        "#,
        user_id,
        on,
        rows.len() as i32,
        unpriced,
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}
/// 指定日以降のキャッシュを丸ごと捨てる。取引の追加・削除・CSV取込から呼ぶ。
/// ポジション単位ではなくユーザー×日で切るのは、
/// 「日の一部だけ欠けた状態」を作らないため（snapshot_days との整合が壊れる）。
pub async fn invalidate_from(
    conn: &mut PgConnection,
    user_id: Uuid,
    from: NaiveDate,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM daily_snapshots WHERE user_id = $1 AND snapshot_on >= $2",
        user_id,
        from,
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query!(
        "DELETE FROM snapshot_days WHERE user_id = $1 AND snapshot_on >= $2",
        user_id,
        from,
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}
