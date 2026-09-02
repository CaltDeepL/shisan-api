# タスク#21 保有一覧・評価損益

`GET /holdings` を表示する画面を実装した。銘柄×口座のフラット一覧、通貨ごとの合計サマリ、口座別内訳、口座フィルタと全売却済みの表示切替まで。

## ゴールと完了条件

| # | 完了条件 | 結果 |
|---|---|---|
| 1 | 保有一覧を表示できる | ✅ |
| 2 | 価格未登録の銘柄が `null` 表示で行として残る | ✅ 4列が `—`、銘柄名にバッジ、上部に警告帯 |
| 3 | 口座フィルタが効き、404 を扱える | ✅ 復帰導線つき |
| 4 | 全売却済みの表示切替 | ✅ `include_closed` |
| 5 | 通貨ごとの合計サマリ | ✅ |
| 6 | `tsc --noEmit` / `oxlint` / `vite build` パス | ✅ |

## 成果物

web/src/api/holdings.ts # 型の re-export、listHoldings、isPriced
web/src/features/holdings/queries.ts # useHoldings
web/src/features/holdings/format.ts # 表示整形
web/src/features/holdings/HoldingsPage.tsx
web/src/features/holdings/SummarySection.tsx


`App.tsx` に `/holdings` を、`AppLayout.tsx` の `navItems` に「保有一覧」を追加。

## 設計判断

### 比率とパーセントが混在しているので関数名で区別する

`holdings` の `unrealized_pnl_rate` は**比率**（`0.05` = +5%）だが、`allocation` の `ratio` は最初から**パーセント値**（`33.34`）。同じアプリ内で単位が違う。

`formatRatioAsPercent()` という名前にして100倍をこの関数に閉じ込めた。#22 で allocation を扱うときは `formatPercent()`（100倍しない）を別に作る。取り違えれば数値が100倍ずれるが、名前が違えばレビューで気づける。

### 評価損益は引き算で求めない

`Totals.book_value` は「価格未登録の銘柄も含む」全保有の簿価だが、`market_value` は「価格のある銘柄のみ」。母集団が違うので `market_value - book_value` は評価損益にならない（未評価銘柄の簿価が損失として混ざる）。

サーバーの `unrealized_pnl` をそのまま表示する。簿価には「（未評価分を含む）」の注記を出して、差が損益と一致しない理由をその場で読めるようにした。

なお `Totals` に `priced_book_value`（騰落率の分母）は無いので、そもそもフロントでは正しい引き算ができない。

### null 5フィールドの判定は `price` の1点だけ

`price` / `priced_on` / `market_value` / `unrealized_pnl` / `unrealized_pnl_rate` はサーバー契約上「価格未登録なら5つとも null」だが、生成型では独立した `| null` になる。

`isPriced()` を型ガードとして用意し、判定は `price !== null` のみ。他4つは型アサーションで連れてくる。契約が変わったときに直す場所を1箇所に閉じ込めるため。実際には各セルの `format*()` が null を `—` に倒すので、`isPriced` はバッジ判定でしか使っていない。

### 未評価件数はサーバー集計を使う

当初 `holdings.filter(h => !isPriced(h)).length` で数えていたが、`summary.unpriced_count` が返ることが分かったので差し替えた。フィルタ適用時の数え方をフロントとサーバーで二重管理しない。

### フィルタUIはエラー・ローディングの外に置く

存在しない口座を指定すると 404 になる。フィルタが `HoldingsContent` の中にあると、エラー時にセレクトごと消えて「すべて」に戻す手段が失われる。

### `key` は `account_id:asset_id` の複合

同一銘柄を複数口座で保有しうるので `asset_id` 単独では衝突する。

### 一覧に出さなかった列

- `realized_pnl`: 既定で保有ゼロ行が消えるため、行に出しても見えなくなる値。サマリには出している
- `currency`: 行ごとに散らすより通貨カードで区切るほうが誤読が少ない

### 並び順はサーバーに任せる

`holdings_service.rs` が口座名昇順 → シンボル昇順でソートしている。フロントで再ソートすると二重管理になるので、`openapi` の description に順序を明記して依存する形にした（⓪で追記）。

## ⓪ 事前に直したバックエンドの不具合

### `operationId` の重複（26箇所）

`#[utoipa::path]` に `operation_id` を明示していなかったため、ハンドラの素の関数名がそのまま出ていた。`list` が accounts / holdings / transactions で、`create` / `delete` が accounts / transactions で衝突。

`operationId` は OpenAPI 仕様上ドキュメント全体で一意でなければならない。生成された `schema.d.ts` の `operations` インターフェースで型が実際に上書きされており、`operations["list"]` は最後に生成された transactions の型に潰れていた。`tsconfig.app.json` の `skipLibCheck: true` のせいでビルドでは表面化していなかった。

`{動詞}_{単数リソース}` の規約で全26箇所に明示指定。`login` / `register` / `me` / `health` はリソース名を持たない単発アクションなので動詞のみ。

再発防止に `tests/openapi_test.rs` へ `operation_ids_are_unique` を追加。`ApiDoc::openapi()` の型を触ると utoipa のバージョン差で壊れるので、既存テストと同じ「`/openapi.json` を HTTP で取得して `serde_json::Value` で検査」する方式にした。

`skipLibCheck` は外していない。代わりに生成物だけを個別にチェックする `check:schema` スクリプトを追加した（TypeScript 6.0.2 では `tsconfig.json` とコマンドラインの files 指定が両立しないので `--ignoreConfig` が要る）。

### ドキュメントと実装のズレ

- `list_holdings` の doc コメントに `include_unpriced` / `include_zero` という実在しないクエリパラメータが残っていた
- `holdings.rs` の `#[utoipa::path]` に `operation_id` が2回重複指定されていた（値が同じなのでビルドは通っていた）

## つまずいた点と教訓

| 事象 | 原因 | 教訓 |
|---|---|---|
| `operations["list_holdings"]` が存在しない | `operation_id` 未指定 | 生成型の `operations` を使う前に、`operationId` の一意性を確認する |
| `skipLibCheck` が型衝突を隠していた | 生成物も lib 扱い | 生成された型定義は個別に strict チェックする |
| `format.ts` の型が実データと合わない | 生成型の nullable は `string \| null \| undefined` | OpenAPI の `required` に無いフィールドは `undefined` も来る |
| `NonNullable<...>` の `<` 抜け | タイプミス | — |

## 発見したデータ不整合（#21 のバグではない）

ブルボン（2208、株式）の `price_unit` が 10000 で登録されている疑い。100株 × 2,500円の簿価が 25 になっている（正しくは 250,000）。`price_unit` は「N単位あたりの価格」で、株式は 1、投信は 10000。#20 でも同じ取り違えがあった。

三菱の TEST-MF は `12 × 1.00 ÷ 10000 = 0.0012` で、小数0桁の丸めにより簿価が `0` と表示される。テストデータの端数取引によるもので、実務上ありえない値。`formatMoney` に「非ゼロだが0に丸まる場合は `<1`」を足す案は保留（根本原因を隠すため）。

## 次タスクへの引き継ぎ

- **#22（グラフ）**: `formatPercent()`（100倍しない）を追加する。`allocation` の `ratio` 用。`operations` 経由でクエリパラメータ型を取れるようになったので、`from` / `to` / `group_by` / `granularity` は生成型から導出する
- **フィルタ状態のURL反映**: 今回は見送り。#22 で期間指定が入るときにまとめて検討する
- **`pages/` と `features/` の混在**: #19 までは `pages/`、#20 以降は `features/`。フロント完了後（#24 のあと）の整理課題リストに追加

## 再現コマンド

```bash
# バックエンド（⓪の変更を反映）
cd ~/workspace/shisan-api/asset-log
cargo test --test openapi_test
cargo fmt && cargo clippy && cargo test

# フロント
cd ~/workspace/shisan-api/web
npm run gen:api
npm run typecheck && npx oxlint && npm run build
npm run check:schema
npm run dev   # http://localhost:5173/holdings
```