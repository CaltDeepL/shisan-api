# タスク#13 CSVインポート（取引履歴の一括取込）

## 概要

証券会社等からエクスポートした取引履歴CSVを一括取り込みするエンドポイントを追加した。
同じCSVを2回取り込んでも資産が二重計上されないことを最重要要件とし、`external_id` による
重複排除を第一キーに据えている。

## エンドポイント

| メソッド | パス | 用途 |
|---|---|---|
| POST | `/import/transactions/dry-run` | 検証のみ。DBに一切書き込まない |
| POST | `/import/transactions` | 本登録。1トランザクションで全行を処理 |

リクエストは multipart ではなく JSON body。

```json
{ "csv_content": "account,symbol,kind,...\n特定,7203,buy,..." }
```

multipart を選ばなかった理由は、既存の統合テストヘルパー（`tests/common/mod.rs`）が
JSON body 前提で組まれており、テスト側の作り直しコストに見合う利点がなかったため。
将来ファイルアップロードUIを付ける段階で multipart を追加する余地は残している。

## CSVフォーマット

固定フォーマット1種類のみ。証券会社ごとの形式差異はスコープ外とした。

```csv
account,symbol,kind,quantity,price,fee,traded_at,note,external_id
特定,7203,buy,100,2500,0,2024-01-15,,ext-001
```

| 列 | 内容 | 備考 |
|---|---|---|
| `account` | 口座名 | 完全一致で `accounts.name` を検索 |
| `symbol` | 銘柄コード | 大文字小文字を無視して `assets.symbol` を検索 |
| `kind` | `buy` / `sell` | `TradeKind` の serde 表現に一致 |
| `quantity` | 数量 | 正の値のみ |
| `price` | 単価 | 0以上 |
| `fee` | 手数料 | 0以上 |
| `traded_at` | 約定日 | `YYYY-MM-DD` |
| `note` | メモ | 空欄は `NULL` |
| `external_id` | 外部取引ID | 空欄可。あれば重複判定の第一キー |

`note` と `external_id` は空文字を `None` に落とすため、`empty_string_as_none` を
`deserialize_with` に指定している。CSVの空欄は空文字として届くので、これを挟まないと
`Some("")` になってしまう。

## 設計判断

### 1. 未登録の口座・銘柄は自動作成しない

CSV中の口座名・銘柄コードが自DBに存在しない場合、その行を検証エラーとして扱い全体を失敗させる。
自動作成すると、証券会社側の表記ゆれ（「特定口座」「特定」など）がそのまま別レコードとして
増殖し、ポジション計算が口座単位で分裂する。事前に手動登録しておく運用を前提とした。

### 2. 不正行が1件でもあれば全体ロールバック

「有効な行だけ登録する」方式は、部分適用された状態からのリカバリが利用者側で難しい。
CSV全体を1単位として成功/失敗を返す方が、取込のやり直しが単純になる。
検証だけ先に済ませたいケースには dry-run を用意した。

### 3. 重複は「エラー」ではなく「スキップ」

不正行と違い、重複は取込のやり直し時に必ず発生する正常系。エラー扱いにすると
「前回途中まで入った分を手で消してから再実行」という運用になってしまう。
スキップして件数だけ報告する形にし、同一CSVの再投入を冪等にした。

### 4. 重複判定の2段構え

- **第一キー**: `external_id` の一致（`user_id` スコープ）
- **フォールバック**: `external_id` が空の行のみ、
  `(account_id, asset_id, kind, quantity, price, traded_at)` の複合一致

`external_id` のUNIQUE制約を `user_id` スコープにしたのは、証券会社をまたぐと
取引IDが衝突しうるため。口座スコープにしなかったのは、同一証券会社の複数口座間で
同じ取引IDが振られることは無いという前提による。

### 5. 売却超過の検証をCSV取込でも行う

単純にループINSERTするだけだと、既存の単体登録（`handler/transactions.rs::create`）が
持っている売却超過チェックがCSV経路では効かず、CSV経由なら保有数量を超える売りが
通ってしまう穴ができる。

CSVでは各行を単独で検証するのでは足りない。同一ポジションに複数取引が含まれる場合、
CSV全体を時系列に並べた結果で判定する必要があるため、以下の順序にした。

```
1. 全行を検証（形式・所有・重複）
2. 影響を受ける (account_id, asset_id) をソートして収集
3. トランザクション開始
4. ソート順にアドバイザリロックを取得
5. 全行をINSERT
6. ポジションごとに fetch_trades → build_holding で再検証
7. 1組でも Oversell なら rollback
8. 全組成功で commit
```

ロック取得をソート順にしたのは、複数ポジションを同時にロックする際のデッドロック回避のため。
既存の単体登録は常に1ポジションしかロックしないため、この問題は今回初めて発生した。

`build_holding` は `(traded_at, created_at, id)` 順で畳み込むので、CSV内の行順ではなく
DB取得順に任せている。

### 6. `price_unit` の取得元

当初 `fetch_position_context` を呼ぼうとしたが、検証フェーズで `asset_repo::find_by_symbol`
が返す `Asset` に `price_unit` が含まれているため、それを `ParsedRow` に保持して使い回す形にした。
DB往復が1回減る。

### 7. `position_error` の共通化

売却超過時のエラーメッセージを単体登録とCSV取込で揃えるため、
`handler/transactions.rs` 内の private 関数 `position_error` を廃止し、
`error.rs` に `impl From<PositionError> for AppError` を追加した。
呼び出し側は `err.into()` で済む。

### 8. 検証エラーのレスポンス形式

既存の `AppError` は RFC 9457 problem+json で単一の `FieldError` を返す形式だが、
CSV取込は「何行目が、なぜ駄目か」を複数返す必要がある。
既存形式に無理に合わせるより、`ImportReport` をそのまま 422 で返す方が素直と判断した。

```json
{
  "total_rows": 2,
  "to_insert": 1,
  "to_skip_duplicate": 0,
  "errors": [{ "row": 2, "message": "口座が見つかりません: 存在しない口座" }]
}
```

この結果、`/import/transactions` の 422 だけ `content-type: application/json` となり、
他エンドポイントの `application/problem+json` と揃っていない。
統一するなら `AppError` に複数行エラーを持てる variant を追加する必要があり、今回は見送った。

## 変更ファイル

| ファイル | 内容 |
|---|---|
| `migrations/00XX_add_transactions_external_id.sql` | 新規。`external_id` 列＋部分UNIQUE索引 |
| `Cargo.toml` | `csv = "1"` 追加 |
| `src/error.rs` | `From<PositionError> for AppError` 追加 |
| `src/repository/account_repo.rs` | `find_by_name` 追加 |
| `src/repository/asset_repo.rs` | `find_by_symbol` 追加 |
| `src/repository/transaction_repo.rs` | `external_id` をstruct・全SQLに追加、`find_duplicate` 追加 |
| `src/handler/transactions.rs` | `external_id: None` 追加、`position_error` 削除 |
| `src/service/import_service.rs` | 新規 |
| `src/handler/import.rs` | 新規 |
| `src/handler/mod.rs` | `pub mod import;` |
| `src/service/mod.rs` | `pub mod import_service;` |
| `src/lib.rs` | 2ルート追加 |
| `tests/import_test.rs` | 新規。9ケース |

### マイグレーション

```sql
ALTER TABLE transactions
    ADD COLUMN external_id text;

CREATE UNIQUE INDEX transactions_user_external_id_key
    ON transactions (user_id, external_id)
    WHERE external_id IS NOT NULL;
```

部分索引にしているのは、`external_id` が NULL の行同士は重複とみなさないため。
PostgreSQL では NULL 同士は UNIQUE 制約上等価とならないので厳密には省略可能だが、
索引サイズを抑える意味で `WHERE` 句を付けている。

## 詰まった点と原因

### 括弧のズレによる連鎖エラー

`handler/transactions.rs` の `create` を部分編集した際、`if let` ブロックの閉じ括弧が
1つ欠けた。その結果 `tx.commit()` 以降が `if` の内側に飲み込まれ、
後続の `list` / `show` が `create` のネストした関数扱いになり、
`lib.rs` から「`handler::transactions::list` が見つからない」という
一見無関係なエラーが出た。

修正時に今度は逆に `}` が1つ余り、`show` の直後で
「unexpected closing delimiter」になった。

**教訓**: 関数内部を書き換えるときは部分編集ではなく関数全体を差し替える。
エラーが「別の関数が見つからない」形で出たら、まず括弧の対応を疑う。

同じ事象が `service/import_service.rs` でも発生した（for ループの閉じ括弧欠落）。

### `pub mod` 宣言と実ファイルの不整合

`handler/mod.rs` には `pub mod import;` があるのに `handler/import.rs` が
存在しない状態になっていた。エディタの診断表示が古いキャッシュを返しており、
問題なしと表示されていたため気付くのが遅れた。
`cargo check` を実際に走らせて `error[E0583]: file not found for module` を
確認して初めて確定した。

**教訓**: 診断表示と `cargo check` が食い違ったら `cargo check` を信じる。
これまでは「宣言忘れ」が5回だったが、今回は「宣言はあるがファイルが無い」という逆パターン。
チェック観点を「宣言とファイルの両方が存在するか」に広げる。

### 変更の適用漏れが積み重なった

以下が順に「コードは提示されたが未適用」の状態で発覚した。

1. `NewTransaction` / `Transaction` の `external_id` フィールド
2. `insert` / `find_by_id` / `list` の SQL への `external_id` 追加
3. マイグレーション自体の未適用（`sqlx migrate run` 未実行）
4. `find_by_name` / `find_by_symbol` / `find_duplicate` の3関数
5. `handler/import.rs` そのもの

`cargo check` のエラーが1つ潰れるごとに次が出る形になり、往復が増えた。

**教訓**: 複数ファイルにまたがる変更は、チェックリストを作って
全部適用してから一度に `cargo check` する。

### DB側の変更を忘れてコンパイルエラー

`sqlx::query_as!` はコンパイル時に実DBのスキーマを参照するため、
コード側だけ `external_id` を追加しても
`column "external_id" of relation "transactions" does not exist` で落ちる。
`sqlx migrate run` を先に実行する必要がある。

### 動作確認時の手順ミス

- `$TOKEN` がシェルセッションをまたいで引き継がれず 401
- パスワードが12文字未満で `register` が 422、その結果 `login` も失敗
- `python3 -c` のJSONキー名 `['access_token']` をトークン値そのものに置換してしまった
- 特定口座は `withholding` が必須（CHECK制約）で 422

## 動作確認結果

| ケース | 結果 |
|---|---|
| dry-run | 200 / `to_insert: 1`、DBは空のまま |
| 本登録 | 200 / `inserted: 1` |
| 同一CSV再投入（external_idあり） | 200 / `skipped_duplicate: 1` |
| 同一CSV再投入（external_idなし・内容一致） | 200 / `skipped_duplicate: 1` |
| 売却超過（200株 > 保有150株） | 422 / 単体登録と同一メッセージ |
| 未登録口座 | 422 / `errors` 1件 |
| 複数行の一部不正 | 422 / 有効行も未登録（ロールバック確認済み） |

`cargo test` 全79件・`cargo clippy -- -D warnings` ともにパス。

## 残課題

- 422 レスポンスの content-type が他エンドポイントと不統一（上記「設計判断8」参照）
- 残高スナップショットの取込は未対応。取引履歴のみ
- 証券会社ごとのCSVフォーマット差異への対応は未着手
- 大量行（数万行）を投げた場合のメモリ・タイムアウト挙動は未検証。
  現状は全行をメモリに載せてから処理している
- `find_by_name` / `find_by_symbol` / `find_duplicate` を行ごとに呼ぶため、
  N行のCSVで最大3N回のDB往復が発生する。行数が増えたら一括取得への変更を検討

## 次のタスク

タスク#14