# タスク#23 CSVインポート画面

## 目的

取引の一括登録UIを追加する。バックエンド（#13）の `POST /import/transactions/dry-run` と
`POST /import/transactions` を、検証 → 本登録の2段フローとして画面に落とす。

## 完了条件

| # | 条件 | 結果 |
|---|---|---|
| 1 | ファイル選択・テキスト貼り付けの両方からCSVを読み込める | ✅ |
| 2 | dry-runの検証エラーが行番号つきで一覧表示される | ✅ |
| 3 | 検証OKなら件数サマリが出て本登録ボタンが有効になる | ✅ |
| 4 | CSV変更で検証結果が破棄され、本登録できない状態に戻る | ✅ |
| 5 | 本登録後に取引一覧・保有・分析のクエリが無効化される | ✅ |
| 6 | `tsc -b` / `oxlint` / `vite build` パス | ✅ |

## ファイル一覧

| パス | 内容 |
|---|---|
| `web/src/api/import.ts` | 型・CSV読み込み・`dryRunImport` / `runImport`・`ImportCounts` |
| `web/src/features/import/queries.ts` | `useDryRunImport` / `useRunImport`（invalidate含む） |
| `web/src/features/import/preview.ts` | 表示用の簡易CSVパース、`row` → メッセージのMap化 |
| `web/src/features/import/ImportPage.tsx` | 画面本体（入力 → 検証結果 → 取込結果の3ステート） |
| `web/src/features/import/ImportErrorTable.tsx` | エラー一覧（行番号・メッセージ・登録導線） |
| `web/src/features/import/CsvPreview.tsx` | プレビュー表。エラー行を赤でハイライト |
| `web/src/App.tsx` | `/import` ルート追加（`/analytics` の直後） |
| `web/src/routes/AppLayout.tsx` | `navItems` に「CSVインポート」追加 |

## 設計判断

### 1. 422がproblem+jsonでない契約を、どこで吸収するか

`/import/transactions` の422は `AppError` → `ProblemDetails` の通常経路を通らず、
`ImportReport` を生JSONで返す（#13の設計判断。他エンドポイントの `application/problem+json` と不統一）。

一方 `client.ts` の `readProblem` は content-type に `json` が含まれれば `as ProblemDetails` と
型アサーションで決め打ちする。したがって `ApiError.problem` の静的型は常に `ProblemDetails` だが、
このエンドポイントに限り実行時の中身は `ImportReport` になる。**型が嘘をついている状態。**

**採用：`api/import.ts` の `unwrapReport` でランタイムガードして吸収する。**

```ts
function unwrapReport(err: unknown): ImportReport | null {
  if (!(err instanceof ApiError)) return null;
  if (err.status !== 422) return null;
  return isImportReport(err.problem) ? err.problem : null;
}
```

`isImportReport` は `value: unknown` を受けて `total_rows` 等の実フィールドをダックタイピング判定するため、
`err.problem` の不正確な静的型に依存しない。`status === 422` の決め打ちではなく実体を見ているので、
将来 `AppError` 側に複数行エラーのvariantが入っても静かに壊れない。

**却下した案：`ApiError.problem` を `ProblemDetails | ImportReport` のunionにする。**
`ApiError` は全エンドポイント共通のクラスであり、1エンドポイントの特殊事情のために
`err.problem.title` を読む既存の全画面（#18〜#22）へ絞り込みを波及させることになる。

**却下した案：本登録の422を `onError` で処理する。**
dry-runの結果表示と本登録の失敗表示は同じ `ImportReport` なのに、描画経路が二分してしまう。
`runImport` は `{ kind: "inserted" | "rejected" }` の判別可能ユニオンを返し、
422を例外にしないことで、画面側は1つの `report` stateだけを見ればよくなる。

なお `readProblem` 自体の修正案（problem+json形式でない本文を `raw` に分離する）は、
import固有ではない一般的な改善だが今回は見送り、#19の残差分リストに追加した。

### 2. 検証結果の破棄を `setCsv` に集約

CSVを編集したのに古い検証結果が残っていると、**別の内容を検証済みとして本登録できてしまう。**
これは全行ロールバックでも防げない（内容そのものが正しく入ってしまうため）。

`ImportPage` では state更新を `setCsv` に一本化し、その中で
`setReport(null)` / `setDone(null)` / `dryRun.reset()` / `run.reset()` をまとめて行う。
`setCsvRaw` を直接呼ぶ箇所は `setCsv` の内部のみ。

### 3. 件数フィールドの名前ゆれを境界で吸収

検証時は `to_insert` / `to_skip_duplicate`、本登録成功時は `inserted` / `skipped_duplicate` と
フィールド名が異なる（`to_` の有無も違う）。`ImportCounts` に寄せて画面側からは消した。

### 4. 行番号の換算を `preview.ts` に閉じる

`ImportRowError.row` はヘッダ行を除く1始まり。プレビューの配列インデックスとは1ずれる。
`+1` / `-1` が複数箇所に散ると必ずズレるため、`parsePreview` の中だけで対応付ける。

### 5. その他

- **文字コードは明示のセレクト**（UTF-8既定 / Shift_JIS）。自動判定は誤ったときに原因が追えない
- **サイズ・行数の上限をフロントでチェック**してから送信（Render無料枠でのタイムアウト回避）
- **口座・銘柄は自動作成しない**（#13の契約）。エラー表から `/accounts` `/assets` への導線を出す
- **`errors` が空でも `to_insert === 0` なら本登録ボタンは無効**（全件重複のケース）

## つまずいた点

### テストCSVのヘッダ列名を間違えた

`account_name` と書いたため、csvクレートのデシリアライズ段階で全行が落ち、
`CSV解析エラー: ... missing field 'account'` が2件出た。
正しい列名は `account`。画面のバグではなく入力側の誤り。
**教訓：テンプレートダウンロード（`CSV_TEMPLATE_HEADER`）を起点にすれば踏まない。**
この経験から、プレースホルダにもヘッダ行とサンプル行を入れてある。

### 存在しない `countsFromReport` をimportしていた

`ImportPage.tsx` が `api/import.ts` に無い関数をimportしていた（呼び出し箇所も無い不要import）。
削除して解消。

### Dockerデーモン停止によりAPIが落ちていた

検証中に `docker.sock` へ接続できず 8080 が listen していない状態になっていた。
Docker Desktop起動 → `docker compose up -d` で復旧。

## 検証結果

| シナリオ | 結果 |
|---|---|
| 存在しない口座を含むCSVを検証 | エラー表に行番号、プレビュー該当行が赤 |
| 全行正常なCSVを検証 → 本登録 | 完了表示、`/transactions` に反映 |
| 検証成功後にテキストを1文字変更 | サマリ・本登録ボタンが消え入力状態に戻る |
| 同じCSVを再度検証 | `to_insert` 0、`to_skip_duplicate` に全件 |

ビルドは成功。`dist/assets/index-*.js` が 742KB（gzip 214KB）で
Viteの500KB超警告が出るが、これは `/analytics` 有効化により recharts が
バンドルへ含まれるようになったため。エラーではない。

## 次タスクへの引き継ぎ

次は **#24（Render Static Site へのデプロイ）**。

**コード分割は #24 の後に回す。** `React.lazy` + `Suspense` でルート単位に分けるのが定石だが、
Static Site の SPA フォールバック設定が未検証の段階で動的importのチャンクを増やすと、
本番で白画面になったときに「デプロイ設定の問題か、チャンク読み込みの問題か」の切り分けが増える。
単一バンドルの素朴な構成をまず本番で通し、動いた状態を基準線にしてから分割を入れる。
`chunkSizeWarningLimit` の引き上げで警告を消すのは不可（次の追加に気付けなくなる）。

### フロント全体完了後にまとめて対応する残差分

| 出所 | 項目 |
|---|---|
| #19 | 編集ダイアログの key 方式 |
| #19 | 409/422 の文言を `problem.detail` 由来にする |
| #19 | 通貨の `<select>` 化 |
| #23 | `readProblem` の `as ProblemDetails` 決め打ち（`raw` 分離案） |
| #23 | ルート単位のコード分割（まず `/analytics`） |

## 再現コマンド

```bash
# API起動
cd ~/workspace/shisan-api && docker compose up -d

# フロント起動
cd ~/workspace/shisan-api/web && npm run dev
# → http://localhost:5173/import

# 検証
cd ~/workspace/shisan-api/web
npx tsc -b
npx oxlint
npm run build
```

### 動作確認用CSV

```csv
account,symbol,kind,quantity,price,fee,traded_at,note,external_id
存在する口座,7203,buy,100,2500,0,2026-09-03,,
存在しない口座,9984,buy,100,5000,0,2026-09-03,,
```

1行目が通り、2行目だけ「口座が見つかりません: 存在しない口座」となる（`to_insert` は 1）。