# タスク#8 取引CRUD（domain を呼ぶ）

## 1. ゴールと完了条件

タスク#7 で作った純粋関数 `domain::position::build_holding` を、DB 越しに呼ぶ最初のタスク。
取引の登録・参照・削除を実装し、**ドメイン制約（売却超過）が HTTP ステータスまで貫通する**ことを保証する。

> **完了条件**: 保有数量超過の売却が 422

着手時に、これを含む9項目の統合テストへ具体化した。

| テスト | 検証内容 |
|---|---|
| `create_and_list_transaction` | 登録→一覧→単体取得、口座・銘柄・期間フィルタ、`user_id` が漏れないこと、`from > to` は422 |
| `oversell_is_rejected` | **保有数量超過の売却 → 422**。ロールバックされ取引が残らない。ちょうどの売却は通る |
| `positions_are_isolated_per_account` | NISA で買った分を特定口座から売れない → 422 |
| `backdated_sell_is_validated` | 過去日への差し込みで後続の売却が成立しなくなる → 422 |
| `rebuy_after_full_sell_is_allowed` | 全売却後の再購入が通り、過去の保有数量は復活しない |
| `delete_recalculates_position` | 買いの削除で後続の売却が成立しなくなる → 422。成立するなら削除できる |
| `other_users_account_or_asset_is_not_found` | 他人の `account_id` / `asset_id` / 取引 → 404、一覧にも出ない |
| `invalid_input_is_rejected` | 数量0・負数・負の価格・未来日・空白のみのメモ → 422 |
| `requires_authentication` | トークン無しで401 |

`accounts_test.rs` 側にも1件追加した。

| テスト | 検証内容 |
|---|---|
| `account_with_transactions_cannot_be_deleted` | 取引が紐づく口座の DELETE → 422（FK違反が500にならないこと）。取引を消せば削除できる |

結果: `transactions_test` 9 passed / `accounts_test` 8 passed。

---

## 2. 設計判断とその根拠

### 2.1 ポジションの単位は (account_id, asset_id)

同じ銘柄を NISA 口座と特定口座の両方で持つのは普通の運用で、**取得単価は口座ごとに独立**する。
口座横断で合算すると、非課税枠の損益と課税口座の損益が混ざり、税務上の意味を失う。
`/holdings`（#9）と allocation（#12）の集計軸もこの単位を前提にする。

### 2.2 「先に書き換えてから畳み込み直す」方式

登録も削除も、**先に INSERT / DELETE してから変更後の全取引を `build_holding` に通し、
`Oversell` ならトランザクションごとロールバック**する。

事前に「今の保有数量 ≧ 売却数量か」を確かめる方式だと、末尾への追加しか守れない。
過去日付への差し込み（後続の売却が超過になる）と、買いの削除（同上）を取りこぼす。
書き換え後の状態をそのまま検証すれば、3ケースが1本の経路で片づく。

### 2.3 排他制御はアドバイザリロック

`SELECT ... FOR UPDATE` は**既存行しかロックできない**ため、同じポジションへの同時 INSERT を防げない。
2つのリクエストが同時に「残10株」を読み、それぞれ8株の売却を通してしまう。

```sql
SELECT pg_advisory_xact_lock(hashtextextended($1::uuid::text || ':' || $2::uuid::text, 0))
```

トランザクション終了で自動解放される。直列化されるのは同じ (account_id, asset_id) だけで、
別銘柄・別口座の登録はブロックされない。

### 2.4 その他

| 論点 | 決定 | 理由 |
|---|---|---|
| 入出金・配当 | 今回はスコープ外。別テーブル `cash_flows`（#11 で追加予定） | `quantity` / `price` の意味が変わり、CHECK 制約が破綻する |
| PATCH | 作らない。訂正は削除→再登録 | 過去取引の書き換えは以降の全ポジションに波及し、検証項目が倍増する |
| `updated_at` | **列もトリガも置かない** | 更新経路が無い列は死に列。将来 PATCH を足すときに3行のマイグレーションで済む。`accounts` / `assets` とは意図的に非対称 |
| `traded_at` | `date` 型。並びは `(traded_at, created_at, id)` | 証券会社CSV（#13）は日付のみ。同日の複数取引は登録順で確定させる |
| `user_id` | `account_id` から辿れるが非正規化して保持 | 一覧クエリを1テーブルで完結させるため。所有確認はハンドラが先に通す |
| FK | `account_id` / `asset_id` とも `ON DELETE RESTRICT` | タスク#5で決めた「取引が紐づく口座の削除は422」を DB 側で担保 |

---

## 3. つまずいた点と教訓

### 3.1 `cargo check` が通っても、コンパイルされているとは限らない

`mod.rs` に `pub mod transaction_repo;` を書き忘れると、ファイルは**黙って無視される**。
`cargo check` は成功し、`query!` マクロも走らない。

> **サイン**: 新規ファイルを追加したのに `cargo check` が1秒未満で終わる。
> `query!` は実DBに接続してクエリを検証するので、本来は数秒かかる。

同じことがルータ登録にも言える。`handler/transactions.rs` を書いて `mod` 宣言もしても、
`lib.rs` の `Router` に `.route()` を足さなければ**コンパイルは通り、実行時に404になる**。
`grep -n "route" src/lib.rs` で登録漏れを確認する習慣をつける。

### 3.2 未来日の判定は JST で行う

```rust
fn today_jst() -> NaiveDate {
    let jst = FixedOffset::east_opt(9 * 3600).expect("固定オフセットは常に有効");
    Utc::now().with_timezone(&jst).date_naive()
}
```

`Utc::now().date_naive()` だと、日本時間の夜9時以降に「今日」の取引を登録したとき、
UTC ではまだ前日のため未来日と判定されて422になる。
`chrono-tz` を入れるほどではないので固定オフセットで足りる。

なお `traded_at <= current_date` の CHECK 制約は書けない（`current_date` が IMMUTABLE でない）。
タスク#6 の `priced_on` と同じ制約で、判定はハンドラ層の責務。

### 3.3 ENUM のバインドはキャストと `as` がセット

```rust
VALUES ($1, $2, $3, $4::trade_kind, ...)
// ...
new.kind as TradeKind,
```

SQL 側の `::trade_kind` と Rust 側の `as TradeKind` は片方だけだと型推論が通らない。
取得側は `kind AS "kind: TradeKind"`。`account_type` / `asset_class` と同じ流儀。

### 3.4 psql の出力は途中で切れていた

`\d transactions` の CHECK 制約は名前順（fee → note → price → quantity）に出る。
`note` の行で貼り付けが切れていたため一瞬「制約が2本足りない」と誤読した。
制約の確認は `\d` より以下が確実。

```sql
SELECT conname, contype, confdeltype FROM pg_constraint
WHERE conrelid = 'transactions'::regclass ORDER BY conname;
```

---

## 4. 成果物

| ファイル | 内容 |
|---|---|
| `migrations/0004_transactions.up.sql` / `.down.sql` | `trade_kind` ENUM + `transactions` |
| `src/domain/position.rs` | `TradeKind` に `sqlx::Type` / `Serialize` / `Deserialize` を追加（計算部分は変更なし） |
| `src/repository/transaction_repo.rs` | `fetch_position_context` / `lock_position` / `fetch_trades` / `insert` / `find_by_id` / `list` / `delete` |
| `src/handler/transactions.rs` | `POST /transactions`、`GET /transactions`、`GET/DELETE /transactions/{id}` |
| `src/lib.rs` | ルータに2ブロック追加 |
| `tests/transactions_test.rs` | 9ケース |
| `tests/accounts_test.rs` | 1ケース追加 |
| `.sqlx/` | クエリキャッシュ7件を追加 |

### エラー対応表に追加した制約名（タスク#3の表）

| 制約名 | SQLSTATE | 応答 |
|---|---|---|
| `transactions_quantity_positive` | 23514 | 422 数量は正の数を指定してください |
| `transactions_price_non_negative` | 23514 | 422 価格は0以上を指定してください |
| `transactions_fee_non_negative` | 23514 | 422 手数料は0以上を指定してください |
| `transactions_note_not_blank` | 23514 | 422 メモは空白のみにできません |
| `transactions_account_id_fkey` | 23503 | 422 取引が登録されている口座は削除できません |

---

## 5. 次タスクへの引き継ぎ（#9 `GET /holdings`）

- `transaction_repo::fetch_trades` はそのまま再利用できる。ただし `/holdings` は
  **全ポジションを一度に**返すので、(account_id, asset_id) ごとの N+1 を避けるクエリが要る。
  1本の SELECT で取引を全件取り、Rust 側で `(account_id, asset_id)` にグループ化して
  `build_holding` を回す形が素直。
- 現在価格は `asset_prices` の**最新日**を引く。価格が1件も無い銘柄をどう返すか
  （`market_value: null` か、評価対象から外すか）は #9 で決める。
- 外貨建て資産の JPY 換算は `analytics_service` の責務（#10 の `FxRateProvider` 待ち）。
  `/holdings` は資産通貨のまま返す。
- `tests/holdings_test.rs.wip` を `.rs` に戻すのは #9。

## 6. 再現コマンド

```bash
cd ~/workspace/shisan-api && docker compose up -d db && cd asset-log

sqlx migrate run
sqlx migrate revert && sqlx migrate run

cargo check --all-targets && cargo sqlx prepare -- --all-targets
cargo test --test transactions_test --test accounts_test
```