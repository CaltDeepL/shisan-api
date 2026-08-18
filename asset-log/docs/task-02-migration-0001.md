# タスク#2 作業メモ: マイグレーション0001（users / accounts）

- **対象**: 資産管理API（asset-log）
- **完了日**: 2026-08-19
- **成果物**: `migrations/0001_init.up.sql`, `migrations/0001_init.down.sql`

---

## 1. ゴールと完了条件

| # | 完了条件 | 結果 |
|---|---|---|
| 1 | `sqlx migrate run` が成功し `_sqlx_migrations` に記録される | OK |
| 2 | `sqlx migrate revert` → `run` の往復が通る | OK（revert 15ms） |
| 3 | `\d accounts` と `\dT+ account_type` が期待通り | OK |

down migration の動作確認を完了条件に含めたのは、後続タスクでスキーマを変更する際に revert が壊れていると詰むため。

---

## 2. 作成したスキーマ

### 2.1 共通トリガー関数

```sql
CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
```

アプリ側で `updated_at` をセットし忘れても DB が保証する。以降の全テーブルで再利用する。

### 2.2 ENUM: `account_type`

| 値 | 意味 |
|---|---|
| `tokutei` | 特定口座 |
| `ippan` | 一般口座 |
| `nisa_tsumitate` | NISA つみたて投資枠 |
| `nisa_growth` | NISA 成長投資枠 |
| `ideco` | iDeCo |
| `bank` | 銀行/証券の待機資金（現金） |

### 2.3 `users`

| 列 | 型 | 制約 |
|---|---|---|
| `id` | UUID | PK, `gen_random_uuid()` |
| `email` | TEXT | NOT NULL |
| `password_hash` | TEXT | NOT NULL |
| `display_name` | TEXT | — |
| `created_at` / `updated_at` | TIMESTAMPTZ | NOT NULL, `now()` |

- `CREATE UNIQUE INDEX users_email_lower_key ON users (lower(email));`
  - 列に UNIQUE を付けず関数インデックスにしたのは、大文字小文字を区別しない一意性を担保するため
- `BEFORE UPDATE` トリガーで `set_updated_at()`

### 2.4 `accounts`

| 列 | 型 | 制約 |
|---|---|---|
| `id` | UUID | PK, `gen_random_uuid()` |
| `user_id` | UUID | NOT NULL, FK → `users(id)` ON DELETE CASCADE |
| `name` | TEXT | NOT NULL |
| `account_type` | `account_type` | NOT NULL |
| `withholding` | BOOLEAN | nullable（特定口座のみ意味を持つ） |
| `institution` | TEXT | — |
| `currency` | CHAR(3) | NOT NULL, DEFAULT `'JPY'` |
| `created_at` / `updated_at` | TIMESTAMPTZ | NOT NULL, `now()` |

制約とインデックス:

- `accounts_currency_format` — `currency ~ '^[A-Z]{3}$'`
- `accounts_name_not_blank` — `length(btrim(name)) > 0`
- `accounts_user_name_key` — UNIQUE (user_id, name)
- `accounts_withholding_only_tokutei` — 後述
- `accounts_user_id_idx` — user_id への btree（「自分の口座一覧」が最頻クエリ）

---

## 3. 設計判断とその理由

### 3.1 主キーを UUID にした

連番の場合、他ユーザーのリソースIDが推測可能になる。PostgreSQL 13 以降は `gen_random_uuid()` が標準搭載なので拡張不要。

### 3.2 時刻を全て TIMESTAMPTZ にした

JST ローカル運用でも、後続タスクで為替APIを叩く際に UTC との突合が発生する。`TIMESTAMP`（タイムゾーンなし）で作ると後から移行するのが高コスト。

### 3.3 NISA を2つの値に分けた

2024年以降の新NISAは「つみたて投資枠」と「成長投資枠」で年間上限額が異なる。非課税枠の消費状況を管理する機能を後から入れる場合、枠ごとに集計できないと実装できない。ENUM への値追加はマイグレーションのコストが高いため、最初から分離した。

### 3.4 源泉徴収区分を ENUM ではなく列にした

**採用案**: `accounts.withholding BOOLEAN` を追加
**却下案**: ENUM を `tokutei_withholding` / `tokutei_no_withholding` に分割

理由:
- 源泉徴収の有無は確定申告の要否に関わるだけで、損益計算式自体は変わらない
- ENUM への値追加・変更はマイグレーションが煩雑
- 列なら後から他の属性（口座番号など）を足す際と同じ扱いで済む

### 3.5 withholding を nullable + CHECK にした

```sql
CONSTRAINT accounts_withholding_only_tokutei CHECK (
    (account_type = 'tokutei' AND withholding IS NOT NULL)
    OR
    (account_type <> 'tokutei' AND withholding IS NULL)
)
```

`NOT NULL DEFAULT false` にしなかった理由: iDeCo 口座に `withholding = false` が入っていると「源泉徴収なしの特定口座」と区別できず、意味のないデータが増える。「NULL = 適用外」を DB レベルで強制することで、Rust 側が `Option<bool>` で受け取れて型が意図を語る。

### 3.6 currency を初期から持たせた

米国株など外貨建て口座を後から扱えるようにするため。`CHAR(3)` + CHECK で ISO 4217 の形式を強制。

---

## 4. Rust 側の対応

```rust
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

impl AccountType {
    /// 非課税口座かどうか（損益計算・税引後リターンの分岐で使う）
    pub fn is_tax_exempt(self) -> bool {
        matches!(self, Self::NisaTsumitate | Self::NisaGrowth | Self::Ideco)
    }
}
```

```rust
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
```

**注意**: `#[sqlx(type_name = "account_type")]` は Postgres 側の ENUM 名と完全一致が必須。ズレると実行時に `unsupported type account_type` で落ちる。

---

## 5. つまずいた点と原因（重要）

### 5.1 `failed to lookup address information`

**症状**: `sqlx migrate run` が名前解決エラーで失敗。

**原因**: `DATABASE_URL` のホストが `db`（compose のサービス名）だった。`db` は compose ネットワーク内部からしか解決できず、ホストの macOS シェルからは解決できない。

**解決**: 実行主体ごとにホスト名を使い分ける。

| 実行主体 | ホスト名 |
|---|---|
| macOS ホスト → db コンテナ | `localhost` |
| api コンテナ → db コンテナ | `db` |
| db コンテナ内 → 自分自身 | `localhost` |

**採用した運用**: `.env` の `DATABASE_URL` はコンテナ用に `db:5432` のまま維持し、CLI 用は別変数にする。

```bash
export SQLX_DB_URL='postgres://assetlog:***@localhost:5432/assetlog'
sqlx migrate run --database-url "$SQLX_DB_URL"
```

`DATABASE_URL` を直接 export すると、compose の変数展開がシェル環境変数を優先するため、`docker compose up` した api コンテナが自分自身の 5432 を見に行って壊れる。変数名を分けるのはこれを避けるため。

### 5.2 `role "user" does not exist`

**原因**: 実際の値は `POSTGRES_USER=assetlog` / `POSTGRES_DB=assetlog`。

**教訓**: `POSTGRES_USER` はボリュームが空の初回起動時にしか適用されない。既存ボリュームがあると compose.yaml を書き換えても無視される。実際の値は `docker compose exec db env | grep POSTGRES` で確認するのが確実。

### 5.3 `no configuration file provided: not found`

**原因**: compose.yaml が無いディレクトリで実行した。

**判明した構成**:

```
~/workspace/shisan-api/          ← compose.yaml（プロジェクト名 shisan-api）
├── compose.yaml
└── asset-log/                   ← Rust クレート（Cargo.toml, migrations/）
```

Compose v2 は compose.yaml が見つかるまで親ディレクトリを遡るため、`docker compose` はどちらからでも動く。一方 `sqlx` と `cargo` はカレントを見るので `asset-log/` で実行する必要がある。

### 5.4 `syntax error at or near ";"`

**原因**: CHECK 制約の末尾で閉じ括弧とセミコロンの順序が逆になっていた。

```sql
-- 誤り
        (account_type <> 'tokutei' AND withholding IS NULL)
);
)

-- 正しい
        (account_type <> 'tokutei' AND withholding IS NULL)
    )
);
```

内側の `)` が CHECK の条件式を閉じ、外側の `)` が CREATE TABLE を閉じ、最後に `;`。Postgres は `);` の時点でテーブル定義が閉じていないまま文が終わったと判断してエラーを出す。

なお sqlx はマイグレーションをトランザクションで包むため、失敗時は自動ロールバックされる。中途半端なテーブルは残らない。

### 5.5 `migration 2 was previously applied but is missing in the resolved migrations`

**原因**: 0002〜0005 の空マイグレーションファイルを先に作っていた。空の SQL は「何も作らず成功」として `_sqlx_migrations` に記録される。その後ファイルを削除したため、記録と実体が不一致になった。

**解決**: `sqlx database reset --database-url "$SQLX_DB_URL"` で記録テーブルごと作り直し。

**教訓（最重要）**:
- 適用済みマイグレーションのファイルは削除・変更してはいけない。中身を書き換えるとチェックサム不一致で全体が停止する
- **空のマイグレーションファイルを先に並べてはいけない**。`sqlx migrate add` は SQL を書く直前に実行する
- 実務では、他人が適用したマイグレーションを pull し忘れたときに同じエラーが出る

---

## 6. compose.yaml の改善点（未対応）

healthcheck がハードコードされている。

```yaml
# 現状
test: ["CMD-SHELL", "pg_isready -U assetlog -d assetlog"]

# 改善案
test: ["CMD-SHELL", "pg_isready -U $$POSTGRES_USER -d $$POSTGRES_DB"]
```

`.env` のユーザー名を変えると healthcheck だけ壊れ、`depends_on: condition: service_healthy` が永久に待機する。`$$` は Compose 自身の変数展開を抑止してコンテナ内シェルに `$` を渡す記法。

---

## 7. 次タスクへの引き継ぎ

タスク#3（AppError 整備）で、本タスクで追加した制約違反を HTTP ステータスにマッピングする。

| Postgres エラーコード | 意味 | HTTP |
|---|---|---|
| `23514` | CHECK 制約違反（withholding 整合性、currency 形式、name 空白） | 422 |
| `23505` | UNIQUE 制約違反（email 重複、口座名重複） | 409 |
| `23503` | 外部キー違反 | 404 または 422 |

`sqlx::Error::Database` から `.code()` を取り出して分岐する。ここを整えておくとタスク#5（口座CRUD）のハンドラが薄く書ける。

---

## 8. 実行コマンド一覧（再現用）

```bash
cd ~/workspace/shisan-api/asset-log

export SQLX_DB_URL='postgres://assetlog:***@localhost:5432/assetlog'

# マイグレーション作成
sqlx migrate add -r 0001_init

# 適用
sqlx migrate run --database-url "$SQLX_DB_URL"
sqlx migrate info --database-url "$SQLX_DB_URL"

# 確認
docker compose exec db psql -U assetlog -d assetlog -c '\d accounts'
docker compose exec db psql -U assetlog -d assetlog -c '\dT+ account_type'

# 往復確認
sqlx migrate revert --database-url "$SQLX_DB_URL"
sqlx migrate run --database-url "$SQLX_DB_URL"
```