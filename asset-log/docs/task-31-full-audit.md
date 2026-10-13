# タスク #31: 全体監査と整合性改善

## 目的

最新のコード、設定、CI、過去のタスク記録を横断し、現在も残っている課題と、実装済みなのに古い docs だけに残っている引き継ぎを分離する。

監査で見つかった問題のうち、仕様変更を伴わず効果が高いものは同じタスクで修正した。README の `Future Work` を公開用の要約、この文書を優先順位と完了条件を含む詳細一覧とする。

過去の `task-XX` にある「次タスクへの引き継ぎ」「残課題」は実装時点のスナップショットであり、この文書で未完了と再判定されていない項目は現在の残タスクとして扱わない。

## 監査時点

- 日付: 2026-09-10
- GitHub `main`: `a346357`（PR #47「全体リファクタリング」のマージ）
- ローカルの元コミット: `a346357`
- GitHub / ローカルの比較結果: ファイル差分なし
- Open PR / Issue: 0件
- PR #47 の CI run #94: success
  - `Backend (Rust)`: success
  - `Frontend (React)`: success

## 監査で見つかり、修正した項目

| 重要度 | 問題 | 対応 |
|---|---|---|
| 高 | 取引・価格・口座などの mutation 後に、一覧だけが失効し、保有評価額や分析結果が古いまま残り得た | React Query のルートキーと失効処理を `web/src/lib/queryKeys.ts` に集約し、関連する holdings / analytics も同時に再取得 |
| 中 | `ApiError` がすべての JSON エラー本文を `ProblemDetails` とみなし、CSV import 422 の `ImportReport` と型契約が食い違っていた | raw body を `unknown` で保持し、表示用 `ProblemDetails` と分離。import 側は型ガード後に raw body を読む形へ変更 |
| 中 | 口座編集の mutation hook が、ダイアログを閉じている間もダミーの空文字 ID を保持していた | ID を mutation 引数へ移し、実行時に確定済み ID を必須化 |
| 中 | 409 の説明文をフロントに重複実装しており、API の文言変更と乖離し得た | `problem.detail` を表示し、サーバーを文言の正本に統一 |
| 中 | `NotFound` に完成済み文章や英語を渡す箇所があり、`が見つかりません` の二重付与・言語混在が起きた | `口座`、`銘柄`、`取引` の名詞だけを渡す規約へ統一し、レスポンス本文を回帰テスト |
| 低 | 口座と銘柄で通貨入力方式・選択肢の定義場所が分かれていた | `web/src/lib/currencies.ts` に JPY / USD を集約し、両画面を select に統一 |
| 低 | DB 制約メッセージ生成に不要な `String` allocation と作業用コメントが残っていた | 静的文言を `&'static str` で返し、不要コメントと空 `Vec` を除去 |
| 低 | README が統合前の `ci-web.yml` / `deploy-web.yml` と古いバンドルサイズを説明していた | `ci.yml` / `deploy.yml` の各1本構成、現在の認証・OpenAPI・キャッシュ処理、実測バンドルサイズへ更新 |

## 実装済みと確認した項目

以下は過去 docs に未完了または引き継ぎとして残る記述があるが、現行コードでは完了している。

- backend / frontend の CI は `.github/workflows/ci.yml`、両デプロイは `.github/workflows/deploy.yml` へ統合済み
- API 起動時に `sqlx::migrate!()` を実行し、`build.rs` で migration 変更時の再ビルドも保証済み
- 登録パスワードは Unicode 文字数・UTF-8 byte 数の上限と common password denylist を実装済み
- Argon2 の新旧 PHC 互換、salt、JWT の期限切れ・改ざん・異なる署名鍵・アルゴリズム不一致をテスト済み
- 認証付き API の 401 とローカル期限切れセッションの破棄を一元化済み
- 全 documented 4xx / 5xx の OpenAPI schema を検証し、CSV import 422 の `ImportReport` だけを明示的な例外として管理済み
- `cargo audit` / `npm audit` / `cargo sqlx prepare --check` / OpenAPI 生成型同期を統合 CI で検証済み
- Compose の DB healthcheck、Rust toolchain 固定、builder / runtime の Debian 12 統一、Dependabot 設定は完了済み
- production bundle は約190 kB（gzip 約60 kB）で、過去 docs の500 kB警告は再現しない

## 追加した回帰テスト

- 他ユーザーの銘柄を取得した404が `銘柄が見つかりません` を返す
- 削除済み取引の再取得が `取引が見つかりません` を返す

既存の認証・OpenAPIテストも含め、全118件（ユニット21 / 統合97）を通過することを確認した。

## 残課題

### P1: 次に実施

1. **JSON rejection の Problem Details 統一**  
   ハンドラ内のエラーは統一済みだが、不正 JSON や Content-Type 不備は axum の既定レスポンスになる。共通 extractor と実レスポンステストを追加する。
2. **CSV API のサーバー側上限**  
   UI の 1 MiB / 5,000 行制限は迂回できる。request body と行数の上限、413 / 422、境界値テストを追加する。
3. **CSV 検証の N+1 解消**  
   口座・銘柄・既存取引を行ごとに検索している。必要データを一括取得し、DB 往復数が行数に比例しない構造へ変更する。
4. **取引一覧のページング**  
   API の既定100件をフロントが超えられない。cursor と next cursor を API / UI に追加する。
5. **認証エンドポイントのレート制限と復旧導線**  
   IP / アカウント単位の制限、メール確認、期限付き復旧トークン、全セッション失効を一体で設計する。
6. **OpenAPI 型生成ツールの再現性**  
   `npx openapi-typescript` が実行時の最新版を取得する。現行 `openapi-typescript 7.13.0` は TypeScript 7.0.2 を peer dependency として許容しないため、`--force` は使わず、TS7対応版または別生成器を選定して固定する。

### P2: 規模や利用者が増える前に実施

1. snapshot job の実行時間・件数メトリクスを取り、必要なら 202 + 非同期 job または user 単位へ分割
2. snapshot に使用した為替レートの日付・鮮度を保存し、現在常に false の `fx_stale` を正しく伝播
3. `fetch_target_dates` と `fetch_price_grid` に重複する analytics の日付条件を共通化
4. 証券会社別 CSV adapter と、取引履歴ではない残高スナップショットの専用 import 経路
5. NISA の年間枠・生涯枠・売却翌年の簿価再利用を取得価額ベースで追跡
6. 配当、入出金、源泉税、株式分割・併合を buy / sell と別イベントとして記録
7. 監査履歴と全データの export / restore
8. Google OIDC または Passkey / WebAuthn と、HttpOnly Cookie / refresh token を含むセッション方式の再設計
9. 現在10件の common password denylistを、大規模漏えいパスワード corpus と更新運用へ拡張

NISA と認証方式の設計時は、[金融庁「NISAを知る」](https://www.fsa.go.jp/policy/nisa2/know/index.html)、[NIST SP 800-63B-4: Passwords](https://pages.nist.gov/800-63-4/sp800-63b/passwords/)、[Syncable Authenticators](https://pages.nist.gov/800-63-4/sp800-63b/syncable/)を参照する。

### P3: UX・構成整理

1. 空欄・日付範囲など即時判定可能な入力のクライアント側 validation
2. ダイアログの初期化、開閉、mutation error 処理の共通方針
3. route component を置く `pages/` / `features/` の境界統一
4. 取引・分析フィルタと URL query parameter の同期
5. フロント未使用の detail API を利用するか、公開 API として残す理由を文書化
6. API のコールドスタートを通常の通信中と区別する表示・再試行導線
7. XIRR、TWR、ベンチマーク比較によるパフォーマンス分析

## 検証結果

- GitHub `main` と監査開始時のローカル: `a346357` で一致
- GitHub Open PR / Issue: 0件
- PR #47 CI run #94: Backend / Frontend とも success
- `cargo fmt --all -- --check`: 成功
- `cargo clippy --all-targets -- -D warnings`: 成功（警告0件）
- `cargo test --all-targets`: 118件成功（ユニット21 / 統合97）
- `cargo sqlx prepare --check -- --all-targets`: 成功
- `npm run lint`: 成功
- `npm run check:schema`: 成功
- `npm run build`: 成功（約190 kB / gzip 約60 kB）
- `npm run gen:api`: 成功。生成済み `schema.d.ts` に差分なし
- `cargo audit`: 既知の脆弱性0件。間接依存 `paste 1.0.15` の unmaintained advisory のみ
- `npm audit --audit-level=moderate`: 脆弱性0件
- `docker compose --env-file .env.example config --quiet`: 成功
- `git diff --check`: 成功
- tracked files の代表的な秘密情報パターン検査: 検出0件

## 出典となった過去文書

- `task-13-csv-import.md`: CSV上限、N+1、証券会社別adapter、残高取込
- `task-14-daily_snapshot .md`: snapshot scaling、FX鮮度、analytics SQL
- `task-19-accounts-ui.md` / `task-20-assets-transactions-ui.md`: validation、dialog、ページング
- `task-21-holdings-ui.md`: ディレクトリ境界、フィルタ状態
- `task-24-frontend-deploy.md`: コールドスタートUX
- `task-27-password-policy.md`: denylistの将来拡張
- `task-30-openapi-error-contract.md`: JSON rejection
