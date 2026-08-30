# タスク#19 口座画面（一覧・作成・編集・削除）

## 1. ゴールと完了条件

口座 CRUD をブラウザから操作できるようにする。バックエンドはタスク#5 で完成済みなので、本タスクはフロントエンド（`web/`）が主で、バックエンドは検証中に見つかったドキュメント不整合の修正のみ。

完了条件:

- [x] `/accounts` で口座の一覧が表示され、0件のときは空状態が出る
- [x] 作成ダイアログから口座を登録でき、リロードなしで一覧に反映される
- [x] 種別が特定口座のときだけ源泉徴収の入力が現れる
- [x] 口座名が重複したとき、409 が口座名フィールドのエラーとして表示される
- [x] 編集ダイアログで name / institution / withholding を更新できる
- [x] 金融機関名を空欄にすると `institution: null` が送られ、値が削除される
- [x] 変更が1つも無い状態で保存するとリクエストを送らずに警告を出す
- [x] 削除確認ダイアログから口座を削除できる
- [x] 取引が紐づく口座の削除は 422 になり、専用の文言が出て削除ボタンが無効化される
- [x] `npx tsc -b --force` / `npm run lint` が green

## 2. 設計判断

### 2.1 一覧1ページ＋ダイアログ3つ

**採用**: `/accounts` の一覧ページ1枚に、作成・編集・削除をすべて `<dialog>` で載せる。

**棄却**: `/accounts/new`、`/accounts/:id/edit` の別ルート。

口座は多くても十数件で、フォームの項目数も5つ以下。個別 URL を共有したい要求もない。ルートを増やすと `GET /accounts/{id}` の呼び出しとローディング状態の分岐が必要になり、得られるものに対して実装量が見合わない。

副次的な効果として、`GET /accounts/{id}` をフロントから一度も呼ばない構成になった（編集の初期値は一覧のキャッシュから取る）。API としては残しておくが、現状の画面からは未使用。

### 2.2 PATCH は差分のみ送る

**採用**: `buildAccountPatch(before, values)` で現在値と比較し、変わった項目だけを含むボディを組み立てる。差分が空なら `null` を返し、送信せずに「変更された項目がありません」と表示する。

**棄却**: フォームの全項目を毎回送る方式。

サーバーの `AccountPatch::is_empty()` は空パッチに 400 を返す。全項目送信ならこの 400 は永久に踏まないが、代わりに「何も変えずに保存」が成功扱いになり、`accounts_set_updated_at` トリガで `updated_at` だけが動く。意味のない UPDATE を発行しないほうが正しい。

差分方式では 400 を踏み得るが、それはクライアント側で先に検出できる条件なので、サーバーに問い合わせずに止めている。400 は「フロントのバグが漏れたとき」のセーフティネットとして残る形。

### 2.3 編集画面の withholding は三値にしない

`UpdateAccountRequest` は `institution` と `withholding` の両方が三値（未指定 / null / 値）だが、UI 上で三値が必要なのは `institution` だけ。

理由は `account_type` が PATCH で変更できないこと。DB の CHECK 制約 `accounts_withholding_only_tokutei` は双方向なので:

- `tokutei` の口座 → `withholding` は NOT NULL 必須。編集画面では「必須の boolean」
- それ以外の口座 → `withholding` は NULL 必須。編集画面には項目自体が存在しない

つまり編集中に `withholding` を `null` にする操作も、`null` から値にする操作も、口座の種別が固定である以上ありえない。`buildAccountPatch` は `before.account_type === "tokutei"` のときだけ `withholding` をパッチに載せるので、CHECK 制約違反は構造的に起きない。

作成時（`CreateAccountRequest`）は種別を選べるので、こちらは `isTokutei ? values.withholding : null` で送信時に強制的に潰している。UI の条件レンダーと送信時の潰しの二重で担保。

### 2.4 409 は口座名フィールドに手動で紐づける

`ProblemDetails.errors[]` が埋まるのは 422（バリデーション）のときだけで、409（`accounts_user_name_key` 違反）は `errors` を持たない。そのままだと `ApiError.fieldErrors` が空になり、汎用の `FormError` に「同じ名前の口座が既に存在する」相当の文言が出るだけでどのフィールドの話か分からない。

`status === 409` を口座名の重複と決め打ちして `nameError` に流し込む。口座の 409 は現状 `accounts_user_name_key` しかないので成立するが、将来 UNIQUE 制約が増えたら破綻する。そのときは `problem.detail` か `type` で分岐する必要がある。

上段の汎用 `FormError` は `hasFieldError` で抑制し、二重表示を防いでいる。

### 2.5 削除の 422 は専用文言＋ボタン無効化

取引が紐づく口座の DELETE は FK の `ON DELETE RESTRICT` により SQLSTATE 23503 → 422。汎用のエラー表示だと「なぜ消せないのか」「次に何をすべきか」が伝わらないので、この場合だけ「取引が登録されているため削除できません。先にこの口座の取引をすべて削除してください。」に差し替える。

あわせて削除ボタンを `disabled` にする。同じ操作を繰り返しても結果は変わらず、ユーザーがやるべきことは再試行ではなく取引の削除だから。

404（別タブで既に削除済み）は特別扱いせず、`FormError` の汎用表示（「口座が見つかりません」）に任せた。

## 3. 実装したもの

### 3.1 新規（フロントエンド）

| ファイル | 役割 |
| --- | --- |
| `web/src/api/accounts.ts` | API 呼び出し関数、`schema.d.ts` からの型再エクスポート、`buildAccountPatch` |
| `web/src/features/accounts/labels.ts` | `accountTypeLabels` / `accountTypeOptions` / `withholdingLabel` |
| `web/src/features/accounts/queries.ts` | TanStack Query の hooks（`useAccounts` と3つの mutation） |
| `web/src/features/accounts/CreateAccountDialog.tsx` | 作成ダイアログ |
| `web/src/features/accounts/EditAccountDialog.tsx` | 編集ダイアログ |
| `web/src/features/accounts/DeleteAccountDialog.tsx` | 削除確認ダイアログ |
| `web/src/pages/AccountsPage.tsx` | 一覧ページ（空状態・ローディング・エラー・テーブル） |

### 3.2 変更（フロントエンド）

- `web/src/App.tsx` — `/accounts` ルート追加、`GuestOnly` の閉じタグ欠落を修正（4.1 参照）
- `web/src/routes/AppLayout.tsx` — ダッシュボード／口座のナビリンクを追加
- `web/src/api/schema.d.ts` — `openapi-typescript` で再生成
- `web/package.json` — `typecheck` / `gen:api` スクリプトを追加

### 3.3 変更（バックエンド）

- `asset-log/src/handler/accounts.rs` — DELETE の `#[utoipa::path]` を 409 → 422 に修正（4.2 参照）
- `asset-log/src/domain/account.rs` — 未使用の `ToSchema` derive を削除（4.3 参照）
- `asset-log/docs/openapi.json` — 上記に伴い再生成（`cargo test` の `spec_is_written_to_docs` が副作用で書き出す）

### 3.4 ラベル辞書の型

`accountTypeLabels` を `Record<AccountType, string>` にしているのは、`account_type` ENUM に値を足したときにここでコンパイルエラーを出すため。`domain/account.rs` の `is_tax_exempt` が `matches!` ではなく `match` で全バリアントを列挙しているのと同じ意図。

並び順は `Record` のキー順に依存しないよう `accountTypeOptions` 配列で明示している。ラベルの追加漏れは `Record` が、順序は配列が担保する。

## 4. つまずいた点と教訓

### 4.1 `GuestOnly` の閉じタグ欠落でルート構造が壊れていた

`App.tsx` の `<Route element={<GuestOnly />}>` に閉じタグが無く、`/register` ルートが消え、`RequireAuth` / `AppLayout` 以下が誤って `GuestOnly` の内側にネストされていた。本来「`/login`・`/register` だけが未認証向け、`/`・`/accounts` は認証必須」であるべき構造が壊れていた形。

タスク#18 から潜在していたバグで、`/accounts` を追加して初めて表面化した。

**教訓**: JSX の入れ子構造の誤りは型検査を通過する。`tsc` が green でもルーティングの階層は目視確認が要る。ルートを追加したら、新しいルートだけでなく既存ルートの認証境界も一度確認する。

### 4.2 `#[utoipa::path]` の DELETE が 409、実装は 422

ドキュメントには「取引が紐づいているため削除できない」を 409 と書いていたが、`error.rs` の `From<sqlx::Error>` は `FOREIGN_KEY_VIOLATION`（23503）を `unprocessable`（422）に分類しており、`accounts_test.rs` の `account_with_transactions_cannot_be_deleted` も 422 をアサートして通っていた。アノテーションだけが取り残されていた。

**教訓**: `#[utoipa::path]` のステータスは実装から自動導出されない。エラー分類のマッピングを変えたときにアノテーションが同期しない。統合テストのアサーションが実態の正本であり、OpenAPI はそれに追随させるもの。

フロント実装の前に契約を確認する運用（タスク#6 の教訓から始めたもの）が、今回はドキュメントの誤りを検出する形で機能した。

### 4.3 `domain::Account` の `ToSchema` が死んでいた

レスポンスは `AccountResponse` に変換して返す設計なので、内部モデルの `Account` は `#[utoipa::path]` のどこからも参照されていなかった。`docs/openapi.json` を grep した結果、`"Account"` は0件、`"AccountResponse"` は1件。

**教訓**: DTO 変換して返す設計なら内部モデルに `ToSchema` は不要。同じファイル内の `NewAccount` / `AccountPatch` に付いていないのと非対称だったのが手がかりだった。タスク#5 で `AccountResponse` を分ける前の名残と思われる。生成された `openapi.json` を grep すれば死んだスキーマは機械的に見つかる。

### 4.4 既存ページのスタイル規約を確認せずに書いた

`AccountsPage.tsx` を `className="page-head"` `"empty"` `"muted"` のような素の CSS クラス名で書いてしまい、それらのクラスが存在しないためスタイルが一切当たらなかった。「口座を追加」がボタンに見えず、テキストが縦に並ぶだけの画面になった。

他ページ（`LoginPage` / `RegisterPage`）や `Field.tsx` / `FormError.tsx` は Tailwind のユーティリティで書かれていたので、規約から外れていた。

**教訓**: 新しいページを足す前に、既存の同種ファイルを1つ開いてスタイルの当て方を確認する。`tsc` も lint も通るので、ブラウザで見るまで気付かない類の不整合。

なお `Field.tsx` は `type?: "text" | "email" | "password"` でテキスト系専用のため、種別 select と源泉徴収 radio は `Field` を使わず、同じマークアップ規約（`space-y-1` ＋ `block text-sm font-medium text-slate-700` のラベル ＋ `text-sm text-red-600` のエラー）を手で複製している。select が増えるようなら `Field` を discriminated union で拡張する余地がある。

## 5. 検証手順

### 5.1 前提

```bash
cd ~/workspace/shisan-api
docker compose up -d          # api と db が healthy であること
cd web && npm run dev         # http://localhost:5173
```

### 5.2 ブラウザ操作

`http://localhost:5173/accounts` を開き、以下を順に確認した。

**作成**

1. 0件のとき「まだ口座が登録されていません。」の空状態と「口座を追加」ボタンが出る
2. 口座名 `SBI証券` / 種別 `特定口座` / 源泉徴収 `あり` で作成 → ダイアログが閉じ、リロードなしで一覧に行が増える
3. 種別を `iDeCo` に切り替えると源泉徴収のラジオが消える。作成すると一覧の源泉徴収列が「—」になる
4. `SBI証券` で再度作成 → ダイアログが残り、口座名の下に「同じ名前の口座が既に登録されています」が出る。上段に `FormError` は二重表示されない

**編集**

5. 編集ボタン → ダイアログに既存の値が入っている
6. 口座名だけ変えて保存 → 一覧が更新される
7. 何も変えずに保存 → 「変更された項目がありません。」が出て、Network にリクエストが飛ばない
8. 金融機関名を入れて保存 → 一覧に反映
9. 金融機関名を空欄にして保存 → Network のペイロードが `{"institution":null}` で、一覧が「—」に戻る
10. 特定口座で源泉徴収を「なし」に変更 → 一覧の表示が変わる
11. iDeCo 口座を編集 → 源泉徴収の項目が出ない

**削除**

12. 削除ボタン → 確認ダイアログに口座名と種別が出る
13. キャンセル → 何も起きない
14. 取引が紐づいていない口座を削除 → ダイアログが閉じ、一覧から行が消える
15. 取引が紐づく口座を削除 → 赤いメッセージが出て、削除ボタンが無効化される

### 5.3 15 のための取引データ作成

取引画面は未実装（#20 以降）のため curl で用意する。フィールド名は Swagger UI（`http://localhost:8080/docs`）で確認できる。

```bash
ishidahitomi@ishidas-MacBook-Air web % TOKEN=$(curl -s -X POST http://localhost:8080/auth/login \
  -H 'content-type: application/json' \
  -d '{"email":"h@a","password":"123456789012"}' \
  | jq -r '.access_token')

curl -s -X POST http://localhost:8080/assets \
  -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"symbol":"7203","name":"トヨタ自動車","asset_class":"equity","currency":"JPY"}' | jq

  shidahitomi@ishidas-MacBook-Air web % curl -s http://localhost:8080/accounts \
  -H "authorization: Bearer $TOKEN" | jq

ishidahitomi@ishidas-MacBook-Air web % curl -s -X POST http://localhost:8080/transactions \
  -H "authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' \
  -d '{
    "account_id":"58c9b93b-ff45-4a97-b0a2-3ccd6d25d2ff",
    "asset_id":"5d7dec9a-1264-4c17-bb0c-0a5a92d3f88f",
    "kind":"buy",
    "quantity":"100",
    "price":"2500",
    "fee":"0",
    "traded_at":"2026-08-01"
  }' | jq
```

## 6. 次タスクへの引き継ぎ

### 6.1 再利用できるもの

- `labels.ts` の `accountTypeLabels` は保有・取引・分析の各画面でそのまま使える。同じ形で `assetClassLabels`（`equity` / `etf` / `mutual_fund` / `bond` / `cash` / `other`）と `tradeKindLabels` が必要になる
- ダイアログの構造（`useRef<HTMLDialogElement>` ＋ `open` 相当の props で `showModal` / `close` を切り替え、開くたびに state を初期化）は取引・銘柄でも同じ形が使える
- 409 をフィールドエラーに紐づける処理と `hasFieldError` による二重表示の抑制も同様

### 6.2 検討の余地がある箇所

`EditAccountDialog` は `useUpdateAccount(account?.id ?? "")` と書いており、閉じている間はダミーの空文字 id で `useMutation` を作っている。hooks を条件付きで呼べないための回避。空文字のまま送信されることはない（閉じている＝ `account` が null ＝ フォームが描画されない）が、素直ではない。

代案は2つ:

| 案 | 利点 | 欠点 |
| --- | --- | --- |
| `key={editing?.id}` でダイアログごと作り直す | id が常に確定し、state 初期化も `useState` の初期値で済んで `useEffect` が不要になる | 開閉のたびにアンマウントされるので閉じアニメーションが効かない |
| `mutationFn` を id 引数付きにする | 空文字が消える | `queries.ts` の API が他と不揃いになる |

取引画面でも同じ構造になるので、そこで再検討する。

### 6.3 未着手

- 口座名の必須バリデーションをクライアント側でしていない。空のまま作成するとサーバーの `accounts_name_not_blank` で 422 になる。動作としては正しいが、手前で止めたほうが親切
- `currency` は自由入力。実際に扱うのが JPY と USD だけなら `<select>` にするほうが事故が減る
- `GET /accounts/{id}` はフロントから未使用

## 7. 再現コマンド

```bash
# OpenAPI 型の再生成（バックエンドの契約を変えたとき）
cd ~/workspace/shisan-api/asset-log && cargo test   # docs/openapi.json が更新される
cd ../web && npm run gen:api

# 型検査
npx tsc -b --force

# lint
npm run lint

# 開発サーバー
npm run dev
```

`tsc -b` に `--force` を付けているのは、`.tsbuildinfo` による差分ビルドで `schema.d.ts` の再生成直後に素通りすることがあるため。