# タスク#22: 資産推移・資産配分のグラフ

## ゴールと完了条件

`/analytics` で資産推移（`GET /analytics/asset-history`）と資産配分（`GET /analytics/allocation`）をグラフ表示する。バックエンドは #11・#12 で完成済みのため、今回はフロントの画面実装のみ。

完了条件:

- [x] 期間プリセット5種（1ヶ月 / 3ヶ月 / 6ヶ月 / 1年 / 全期間）が切り替わる
- [x] 推移の分類3種（合計 / 口座種別 / 資産クラス）が切り替わる
- [x] 配分の4軸（資産クラス / 口座種別 / 口座 / 銘柄）が切り替わる
- [x] ローディング・エラー・空データの各状態が表示される
- [x] 375px 幅で崩れない
- [x] `tsc --noEmit` / `oxlint` / `vite build` がクリーン

## 成果物

```
web/src/api/analytics.ts                        getAssetHistory / getAllocation
web/src/features/analytics/queries.ts           期間プリセット・ピボット・整形
web/src/features/analytics/format.ts            formatYen / formatPercent / colorAt ほか
web/src/features/analytics/AssetHistoryChart.tsx
web/src/features/analytics/AllocationChart.tsx
web/src/features/analytics/AnalyticsPage.tsx
```

依存追加: `recharts` v3.10.1

## 設計判断の根拠

### グラフライブラリは Recharts

React 前提で TS 型が同梱され、積み上げエリアとドーナツが同一APIで揃うため。Chart.js + react-chartjs-2、visx も候補だったが、今回必要な表現に対して過剰または記述量が多い。

### `/analytics` 1画面に2セクション

推移と配分は同じ「今の資産を俯瞰する」文脈なので画面を分けない。ただしフェッチは独立させ、片方の失敗や `group_by` の切替が他方を巻き込まないようにした。

### 系列キーに `v:` 接頭辞

`HistorySeries.key` は `group_by=none` で `"total"`、口座・銘柄軸では UUID。Recharts に渡す行オブジェクトは `date` や `unpricedCount` と同じ平坦な名前空間なので、`v:${key}` で隔離して衝突を避けた。簿価は `v:__cost__`。

### 折れ線と積み上げエリアの出し分け

`group_by=none` は折れ線（評価額＋簿価の破線）、それ以外は積み上げエリア。簿価ラインは `group_by` ではなく実際の系列数（`isSingleSeries`）で判定しているため、`account_type` で口座種別が1つしかない場合も積み上げエリアのまま（1系列の面グラフとして成立するので実害なしと判断）。

### `formatPercent` は100倍しない

`AllocationItem.ratio` は最大剰余法で合計がちょうど 100.00 になるパーセント値。一方 #21 の holdings で使う比率は 0〜1。同じ「パーセント表示」でも入力の意味が違うため、`formatRatioAsPercent`（#21）と `formatPercent`（#22）を名前で分けて併存させ、100倍を1箇所に閉じ込める方針を維持した。合計行の `100.00%` は仕様に依拠したハードコード（`ratio` の合算は浮動小数の誤差が出るため）。

### retry の打ち切りは 4xx 全般

holdings では 404（存在しない口座）だけ再試行を諦めていたが、analytics に「存在しない」という概念はない。実際に再試行が無駄になるのは 422（未来日・期間過大など）なので、判定を 4xx 全般に一般化した。「再試行しても変わらないエラーは諦める」という意図は同じで、対象エラーの中身だけ実態に合わせている。

### `placeholderData: keepPreviousData`

期間・分類の切替でグラフが毎回スケルトンに戻るのを防ぐ。holdings と同じ狙い。

### 日付は `toISOString()` を使わない

`resolveRange` はローカル時刻で `YYYY-MM-DD` を組み立てる。`toISOString()` は UTC に寄るため JST 早朝に1日ずれる。また `setMonth` の月末繰り上がり（3/31 の1ヶ月前が 3/2 になる）を `setDate(0)` で補正している。

## つまずいた点と教訓

### Recharts v3 の型差分（3点）

`npm i recharts` で v3.10.1 が入り、v2 想定で書いたコードが型エラーになった。

| 症状 | 対応 |
|---|---|
| `TooltipProps` が `payload` / `label` / `active` を持たない | v3 では `TooltipContentProps` が別の型として該当。差し替え |
| `<Tooltip content={<ChartTooltip />} />` が型的に不成立 | v3 は内部で関数コンポーネントとして呼ぶ前提。`content={(props) => <ChartTooltip {...props} />}` へ |
| `payload[].value` が `ValueType`（`string \| number \| (string\|number)[]`）に拡大 | `formatYen` へ渡す前に配列を弾くガードを追加 |

新規追加の依存で守るべき既存挙動もなかったため、v2 固定ではなく型を合わせる方針を選んだ。

### `<Cell>` は v3 で非推奨（v4 で削除予定）

公式の移行先は `shape` prop。`Cell` を1件ずつ子要素として並べる代わりに、`shape` に渡した関数がセクターごとに `index` 付きで呼ばれるので、標準の `Sector` に `fill` だけ差し込む形にした。配色ロジック（`colorAt`）と見た目は変わらない。

```tsx
shape={(props) => <Sector {...props} fill={colorAt(props.index)} />}
```

### 既存ファイルを読まずに書いたことによるミス（今回の最大の反省点）

新規ファイルを書く際に既存の同種ファイルを確認せず、実在しない前提で書いた箇所が複数出た。

- `../types/schema` からの import → 実在せず。生成先も他の `api/*.ts` 6本も `src/api/schema.d.ts`（`./schema`）を使っている
- `apiGet<T>(path, token)` → `client.ts` に `apiGet` は存在せず、正しくは `apiFetch<T>(path)`。トークンは `setTokenProvider` 経由で自動注入されるので引数で渡さない
- `paths[...]` 起点の型取り → プロジェクトの流儀は `operations["get_asset_history"]` などの operation_id ベース（#21 で明示指定したものをそのまま使える）
- 独自の `AssetHistoryResponse` / `AllocationResponse` 型 → 生成スキーマの `HistoryResult` / `AllocationResult` をそのまま使うのが holdings と同じ流儀
- CSS クラス（`chart-card` 等）を定義 → `index.css` は `@import "tailwindcss";` のみで、#19〜#21 は全画面 Tailwind ユーティリティ直書き
- `export default` → プロジェクト規約は named export
- `./queries` から `formatYen` 等を import → 移設済みで正しくは `./format`
- 全角の `＠`（`"＠/api/analytics"`）でモジュール解決エラー
- `<TooltipContentProps {...props} />` — 型名をそのままコンポーネントとして呼んだ。`AllocationTooltip` 未使用警告の原因でもあった
- `useEffect` × 2 ＋ `cancelled` フラグ ＋ 再試行用 `nonce` の手動実装 → プロジェクトは React Query 前提。`useAssetHistory` / `useAllocation` の2フックに置き換え

**教訓**: asset-log 側で `pub mod` 宣言忘れが通算5回発生したときと同じ構造の失敗。「新規ファイル作成時は作成直後に確認する」という運用は Rust 側だけでなく web 側にも必要で、**新しいファイルを書く前に同種の既存ファイルを1本読む**ことを手順に組み込む。

## 次タスクへの引き継ぎ

- 次はタスク#23
- #19 で残した課題（編集ダイアログの key 方式、409/422 の文言を `problem.detail` 由来にする、通貨の `<select>` 化）は #24 まで終えてからまとめて対応する方針のまま
- `AnalyticsPage` の `App.tsx` 配線（ルート追加と `AppLayout` の `navItems`）が済んでいることを確認すること

## 再現コマンド

```bash
cd web
npm run check:schema
npx tsc --noEmit
npx oxlint
npm run build
```

動作確認の観点:

- 期間プリセット全5種
- 推移の分類3種（合計時のみ簿価の破線が出ること）
- 配分の4軸
- エラー時の再読み込みボタン
- 375px での折り返し