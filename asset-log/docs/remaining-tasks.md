# 残タスク整理（2026-09-10）

過去のタスクメモにある「残課題」「未着手」「今後の課題」を、現在のコードと CI に照らして再確認した一覧です。すでに解消した記述は除外し、今も必要なものだけを優先度順にまとめています。

## 優先度 A — API 契約・データ量対策

| 項目 | 現状 | 完了条件 | 主な出典 |
|---|---|---|---|
| JSON rejection の統一 | ハンドラに入った後のエラーは Problem Details だが、不正 JSON や Content-Type 不備は axum の既定レスポンスになる | 共通 JSON extractor で rejection を `AppError` に変換し、実レスポンスを統合テスト | `task-13-csv-import.md`, `task-14-daily_snapshot .md` |
| CSV のサーバー側上限 | UI は 1 MiB / 5,000 行で制限するが、API を直接呼ぶと上限がない | request body と行数に上限を設け、超過時の 413 / 422 と境界値をテスト | `task-13-csv-import.md` |
| CSV 検証の N+1 解消 | 1行ごとに口座・銘柄・重複を検索し、最大 3N 回 DB を往復 | 必要な口座・銘柄・既存 external ID を一括取得し、クエリ数が行数に比例しない構造へ変更 | `task-13-csv-import.md` |

## 優先度 B — 利用規模とデータ品質

| 項目 | 現状 | 完了条件 | 主な出典 |
|---|---|---|---|
| 取引一覧のページング | API 既定は100件で、フロントは limit / cursor を指定しない | cursor またはページングを API と UI に追加し、100件超も到達可能にする | `task-20-assets-transactions-ui.md` |
| 証券会社別 CSV adapter | 共通フォーマットの取引 CSV のみ対応 | 証券会社ごとの列名・文字コード・取引種別を正規化する adapter と fixture を追加 | `task-13-csv-import.md` |
| 残高スナップショット取込 | 取引履歴の CSV のみ対応 | 取得価額が不明な残高データの扱いを決め、取引データと混同しない import 経路を追加 | `task-13-csv-import.md` |
| snapshot job のスケーリング | 全ユーザーを同期処理。無料枠の timeout は未実測 | 実測とメトリクスを追加し、必要なら 202 + 非同期 job / user 単位分割へ移行 | `task-14-daily_snapshot .md` |
| snapshot の FX 鮮度 | snapshot 経路の `fx_stale` は常に false | 使用レートの日付・鮮度を保存し、レスポンスへ正しく伝播 | `task-14-daily_snapshot .md` |
| analytics SQL の二重管理 | `fetch_target_dates` と `fetch_price_grid` に同じ日付条件がある | 共通 CTE / helper に寄せ、片方だけ変更できない構造にする | `task-14-daily_snapshot .md` |

## 優先度 C — UX・構成整理

| 項目 | 現状 | 完了条件 | 主な出典 |
|---|---|---|---|
| クライアント側必須入力 | 多くのフォームがサーバーの 422 まで送信を許す | server validation を正本に保ちつつ、空欄・日付範囲など即時判定できる項目を送信前に表示 | `task-19-accounts-ui.md`, `task-20-assets-transactions-ui.md` |
| ダイアログ実装の統一 | nullable prop + `useEffect` と key remount が混在 | 開閉・初期化・mutation error の共通方針を決めて揃える | `task-24-frontend-deploy.md` |
| `pages/` / `features/` の境界 | 初期画面は `pages/`、後発画面は `features/` に分散 | route component の配置規約を決め、import 境界を統一 | `task-21-holdings-ui.md` |
| フィルタ状態の URL 同期 | 取引・分析の期間や分類がリロード・共有で失われる | query parameter と UI state を同期 | `task-21-holdings-ui.md` |
| 未使用 detail API の方針 | `GET /accounts/{id}` などをフロントが使わない | 詳細画面で使うか、公開 API として残す理由を文書化する | `task-19-accounts-ui.md`, `task-20-assets-transactions-ui.md` |
| コールドスタート UX | API の初回応答が数十秒かかる場合がある | 起動待ちを通常の通信中と区別し、説明・再試行・進捗表示を追加 | `task-24-frontend-deploy.md` |

## プロダクトロードマップ

| 項目 | 目的 |
|---|---|
| XIRR | 入出金タイミングを考慮した金額加重収益率を提供する |
| Google ログイン（OIDC） | 現行 JWT 認証の上に外部 IdP を追加し、Cookie / refresh token 方針も再検討する |

## 現行コードの確認から追加で提案する機能

以下は過去の task 文書からの転記ではなく、今回のコード確認で不足を確認した追加候補です。

| 優先 | 項目 | 理由・実装の焦点 |
|---|---|---|
| 1 | NISA 枠の消費・再利用管理 | 現状は口座種別を分けて集計できるだけで、年間120万 / 240万円、生涯1,800万円（成長投資枠1,200万円内数）、売却した商品の簿価分を翌年以降に再利用するルールは追跡しない。約定年・取得価額を基準に枠残高を表示する |
| 2 | ログイン試行制限とアカウント復旧 | Argon2 の計算コストを利用した DoS と総当たりを抑えるため、IP / アカウント単位の rate limit、メール確認、期限付き復旧トークン、セッション失効を一体で設計する |
| 3 | Passkey / WebAuthn | 資産データに対するフィッシング耐性を高める。OIDC と競合させず、既存 JWT の発行元となる認証方式として段階導入する |
| 4 | 配当・入出金・税・コーポレートアクション | 現状の取引種別は buy / sell のみ。配当、源泉税、入出金、株式分割・併合を別イベントとして記録し、損益・XIRR の正確性を上げる |
| 5 | 監査履歴とデータ export / restore | 金融データの変更者・変更時刻・変更前後を追跡し、ユーザー自身が全データを退避・復元できるようにする |
| 6 | ベンチマーク比較と TWR | XIRR に加えて入出金の影響を除いた時間加重収益率と指数比較を提供し、運用判断と資金投入判断を分離する |

制度・認証要件の参照先:

- [金融庁「NISAを知る」](https://www.fsa.go.jp/policy/nisa2/know/index.html)
- [NIST SP 800-63B-4: Passwords](https://pages.nist.gov/800-63-4/sp800-63b/passwords/)
- [NIST SP 800-63B-4: Syncable authenticators](https://pages.nist.gov/800-63-4/sp800-63b/syncable/)

## 解消済みとして除外した項目

- Compose の DB healthcheck は環境変数参照へ修正済み
- backend / frontend の CI は `ci.yml`、両デプロイは `deploy.yml` へ統合済み
- API 起動時の `sqlx::migrate!()` は実装済み
- `cargo sqlx prepare --check`、`cargo audit`、`npm audit` は CI に追加済み
- OpenAPI 生成型とコミット済み schema の同期確認は CI に追加済み
- 登録パスワードは Unicode 文字数・byte 数の上限と common password denylist を実装済み
- 全 4xx / 5xx の OpenAPI schema 検証は、CSV import 422 の明示的な例外を含めて実装済み
- Dependabot は Cargo / npm / GitHub Actions の週次更新を設定済み
- 認証付き API の 401 一元検知と logout は実装済み
- JWT の expiry / tamper / wrong secret / algorithm mismatch テストは実装済み
- `ApiError` は Problem Details 以外の JSON 本文も raw body として保持する形へ整理
- 口座 mutation のダミー ID とエラー文言のハードコードは解消
- 変更系 API 後の React Query キャッシュ失効先を共通定義へ集約
- 現在の production bundle は約 190 kB（gzip 約 60 kB）で、以前の 500 kB 警告は再現しないためコード分割は当面の必須タスクから除外
