# タスク#20 銘柄・価格・取引の登録画面

## 1. ゴールと完了条件

銘柄・価格・取引をブラウザから登録できるようにする。バックエンドはタスク#6（銘柄・価格）と#8（取引）で完成済みなので、本タスクはフロントエンド（`web/`）が主。バックエンドは検証中に見つかったエラーメッセージの不整合の修正のみ。

完了条件:

- [x] `/assets` で銘柄の一覧が表示され、0件のときは空状態が出る
- [x] 作成ダイアログから銘柄を登録でき、リロードなしで一覧に反映される
- [x] `price_unit` を空欄にするとサーバー既定（投資信託は10000、それ以外は1）が入る
- [x] `q` によるコード・名称の部分一致検索ができる
- [x] シンボルが重複したとき、409 がコードフィールドのエラーとして表示される
- [x] 編集ダイアログで symbol / name / price_unit を更新できる
- [x] 変更が1つも無い状態で保存するとリクエストを送らずに警告を出す
- [x] 銘柄行から価格を登録でき、同じ日付の再登録で上書きされる（UPSERT）
- [x] 価格履歴が日付の降順で表示され、`source` が見える
- [x] `/transactions` で取引の一覧・作成・削除ができる
- [x] 取引登録がリロードなしで一覧に反映される（ロードマップの完了条件）
- [x] 一覧に口座名・銘柄名が UUID ではなく名前で表示される
- [x] 口座・銘柄・期間でフィルタできる
- [x] 422 のうち `errors[]` を持つものは各項目に、持たないものはフォーム上部に出る
- [x] 削除で 422（以降の売却が保有数量を超える）になったとき、削除ボタンが無効化される
- [x] `npx tsc -b --force` / `npm run lint` が green
- [x] バックエンドの `cargo test` / `clippy` / `fmt` が green

## 2. 設計判断

### 2.1 `price_unit` はサーバー既定に委ね、フロントで補わない

**採用**: フォームの `price_unit` が空欄なら、リクエストボディからキーごと省く。

**棄却**: 資産クラスに応じて `1` / `10000` をフロントで埋めて送る。

`CreateAssetRequest.price_unit` は optional で、「未指定なら資産クラスの既定値」がサーバー側にある。フロントで補うと既定のロジックが二重定義になり、片方だけ変更したときに気づかずズレる。`buildCreateAsset()` で `trim()` が空ならキーを立てない形にした。`currency` も同じ扱い。

検証時に curl で `{"asset_class":"mutual_fund"}` を投げて `"10000"` が返ることを確認し、サーバー既定が実際に効いていることを裏取りしている。

### 2.2 エラーの分岐は `problem.type` ではなく `status` で行う

当初は「`type` は安定した識別子でクライアントはこの値で分岐できる」という OpenAPI の記載を根拠に、`type` ベースで分岐する方針だった。実装を確認して撤回した。

`error.rs` の `error_type()` は次のとおりで、HTTP ステータスの言い換えでしかない。

| type | status | 判別できる情報 |
| --- | --- | --- |
| `/errors/conflict` | 409 | 「何かが重複した」だけ。どの制約かは分からない |
| `/errors/unprocessable-entity` | 422 | 「検証に落ちた」だけ。数量超過も未来日も同じ値 |

`status` 以上の情報が無いため、#19 の口座画面でやっていた `status === 409` 決め打ちは当時できる最善だった。銘柄でも同じく `status === 409` をシンボル重複と決め打ちしている。`assets_user_symbol_key` が唯一の UNIQUE 制約なので現状は成立するが、UNIQUE が増えたら破綻する。

将来 `type` で分岐したいなら、`error_type()` を制約名や業務エラー単位に細分化するバックエンド側の変更が先に必要。

### 2.3 422 は `errors[]` の有無で2系統に分けて出し先を変える

同じ 422 でも、発生源によってボディの形が違う。

| 経路 | ボディ | 出し先 | 例 |
| --- | --- | --- | --- |
| `AppError::field(...)` | `errors[]` に `field` 付き | 各入力欄の下 | 数量が0以下、未来日、メモが空白のみ |
| `AppError::unprocessable(...)` | `detail` のみ | フォーム上部の `FormError` | 売却数量が保有数量を超える |

後者は `domain/position.rs` の `PositionError` が `handler/transactions.rs` で `AppError::unprocessable(err.to_string())` に変換されるもので、`errors[]` を持たない。代わりに `detail` に「売却数量 200.00000000 が保有数量 100.00000000 を超えています」と数値入りの具体的な文言が入る。

そのため `CreateTransactionDialog` では `hasFieldError` が false のときに `FormError` を出す作りが必須になる。`fieldErrors` だけを見ていると売却超過が画面のどこにも出ない。

### 2.4 エラー文言はサーバーの `detail` を使い、フロントで書き直さない

`ApiError` は `problem` を丸ごと保持しているので `apiError.problem.detail` が読める。409 の文言はフロントのハードコードをやめ、`detail` を第一候補・ハードコードをフォールバックにした。

```ts
const symbolError =
  apiError?.status === 409
    ? (apiError.problem.detail ?? "このコードは既に登録されています")
    : fieldErrors.symbol;
```

制約名→メッセージの対応表（#3）が唯一の出所になる。取引の削除 422 も同様で、`detail` の「この取引を削除すると、以降の売却が保有数量を超えます」をそのまま出している。#19 の口座削除では専用文言をフロントに書いたが、あれは二重管理になっている。

### 2.5 編集ダイアログは `key` によるアンマウント方式にする

#19 の引き継ぎ 6.2 に残した検討事項。`EditAccountDialog` は `useUpdateAccount(account?.id ?? "")` と書いており、閉じている間はダミーの空文字 id で `useMutation` を作っていた。

銘柄では呼び出し側を `{editing && <EditAssetDialog key={editing.id} asset={editing} .../>}` にして、開いている間しかマウントされない形にした。

| | 口座（#19 の実装） | 銘柄（本タスク） |
| --- | --- | --- |
| id | `account?.id ?? ""` のダミー | 常に実在する `asset.id` |
| state 初期化 | `useEffect` で毎回リセット | `useState` の初期値だけ |
| useEffect | 開閉の同期に1つ | `showModal()` に1つ（依存配列は空） |
| 欠点 | — | 閉じアニメーションが効かない |

現状どのダイアログにもアニメーションを付けていないので、実質的な欠点は無い。`PriceDialog` / `DeleteTransactionDialog` も同じ方式にした。作成ダイアログだけは `open` boolean の従来方式のまま（親が常にマウントしていて id を持たないため）。

### 2.6 取引に編集画面を作らない

`PATCH /transactions/{id}` が存在しない。#8 で「訂正は削除→再登録」と決めているので、UI もそれに従う。

「訂正」ボタンで削除確認と作成ダイアログを合成する案もあったが、削除が成功して作成が失敗したときに元データが消えたままになる。トランザクション境界がまたげない以上、2操作として見せるほうが安全。

削除確認ダイアログに「訂正したい場合は、削除してから登録し直してください」と明示している。

### 2.7 口座名・銘柄名はフロントで `Map` に引き当てる

`TransactionResponse` は `account_id` / `asset_id` の UUID しか返さない。

**採用**: `GET /accounts` と `GET /assets` を別に取り、`useMemo` で `Map` を作って引く。

**棄却**: バックエンドに JOIN を足して名前を返す。

取引フォームの `<select>` で口座と銘柄の一覧はどのみち必要なので、追加の通信が発生しない。完成済みの #8 に手を入れずに済む。取引は最大500件なので引き当てのコストも問題にならない。

3つのクエリのうち1つでも読み込み中なら一覧を描画しない。UUID が一瞬見える状態を作らないため。

### 2.8 Decimal の入力に `type="number"` を使わない

quantity / price / fee / price_unit は API 上すべて文字列。`type="number"` を挟むと `value` が number に丸められ、`0.00000001` のような数量（`numeric(20,8)`）で精度落ちや指数表記が起きる。`type="text"` + `inputMode="decimal"` で受けて文字列のまま送る。

`Field` コンポーネントに `inputMode` の props を追加した。

### 2.9 通貨は `<select>`、`source` は入力欄を作らない

`currency` は #19 の引き継ぎ 6.3 に「自由入力は事故のもと」と書いた件の実行。`assets/labels.ts` の `currencyOptions`（JPY / USD）から選ぶ形にした。口座画面はまだ自由入力のままなので、次タスク以降で揃える。

`UpsertPricesRequest.source` は入力欄を作らず、常にサーバー既定の `manual` に任せる。将来 Frankfurter などから自動投入したときに、履歴上で手入力と区別できる形が残る。履歴テーブルには表示している。

### 2.10 価格登録後はダイアログを閉じない

価格は複数銘柄・複数日をまとめて入れる作業になりやすい。登録が成功したら入力欄をリセットして履歴に反映し、開いたまま次を入れられるようにした。「N件を登録しました」の表示が成功のフィードバックになる。

## 3. 実装したもの

### フロントエンド

| ファイル | 役割 |
| --- | --- |
| `web/src/api/problem.ts` | 手書きの `FieldError` / `ProblemDetails` を生成型ベースに置換（`ApiError` クラスは変更なし） |
| `web/src/api/assets.ts` | 新規。銘柄・価格の API ラッパー、`buildCreateAsset` / `buildAssetPatch` |
| `web/src/api/transactions.ts` | 新規。取引の API ラッパー、`buildCreateTransaction` |
| `web/src/components/Field.tsx` | `inputMode` props を追加 |
| `web/src/features/assets/labels.ts` | 新規。`assetClassLabels` / `assetClassOptions` / `currencyOptions` |
| `web/src/features/assets/queries.ts` | 新規。`useAssets` / `useCreateAsset` / `useUpdateAsset` / `usePrices` / `useUpsertPrices` |
| `web/src/features/assets/AssetsPage.tsx` | 新規。一覧＋検索。`useDebounced` を内包 |
| `web/src/features/assets/CreateAssetDialog.tsx` | 新規 |
| `web/src/features/assets/EditAssetDialog.tsx` | 新規。`key` 方式 |
| `web/src/features/assets/PriceDialog.tsx` | 新規。登録フォーム＋履歴表示 |
| `web/src/features/transactions/labels.ts` | 新規。`tradeKindLabels` |
| `web/src/features/transactions/queries.ts` | 新規 |
| `web/src/features/transactions/TransactionsPage.tsx` | 新規。一覧＋フィルタ |
| `web/src/features/transactions/CreateTransactionDialog.tsx` | 新規 |
| `web/src/features/transactions/DeleteTransactionDialog.tsx` | 新規 |
| `web/src/App.tsx` | `/assets` `/transactions` のルートを追加 |
| `web/src/routes/AppLayout.tsx` | ナビを新設（ダッシュボード／口座／取引／銘柄） |

### バックエンド

| ファイル | 役割 |
| --- | --- |
| `asset-log/src/handler/assets.rs` | エラーメッセージ7箇所を日本語化 |
| `asset-log/src/handler/prices.rs` | エラーメッセージ4箇所を日本語化 |

## 4. つまずいた点と教訓

### 4.1 `price_unit` の意味を取り違えて登録していた

検証中、株式（トヨタ自動車・森永製菓）に `100`、投資信託（ブルボン）に `1` を入力していた。単元株数（売買単位）と解釈していたため。

**`price_unit` は「価格が何口あたりの値か」を表す。**

- 日本株の株価 2,350円 は1株あたりの値段 → `price_unit` は `1`。単元が100株でも1
- 投信の基準価額 12,345円 は10,000口あたりの値段 → `price_unit` は `10000`

評価額は `数量 × 価格 ÷ price_unit`（#7 の `domain::position`）なので、トヨタを100にすると評価額が100分の1、ブルボンを1にすると10,000倍になる。#21 の保有一覧で数字が合わない原因になるところだった。

ヒント文が「投資信託は10000、それ以外は1」としか書いておらず、何を表す値かを説明していなかったのが原因。「価格が何口あたりの値かを表します」を先頭に追加し、編集ダイアログ側にも同じヒントを付けた。

**教訓**: 既定値だけ書いて意味を書かないヒントは、既定値から外れた入力を誘発する。

### 4.2 HMR で `instanceof ApiError` が false になり、原因を実装側に探しに行った

検証4（数量0）・5（単価-1）で、各入力欄にエラーが出ず、フォーム上部に「HTTP 422」とだけ表示された。`ApiError` のコンストラクタは `super(problem.detail ?? problem.title ?? \`HTTP ${status}\`)` なので、`problem` が空のオブジェクトになっている＝パースに失敗している、と読んで `client.ts` の Content-Type 判定やボディの二重読みを疑った。

**実際はブラウザをリロードしたら直った。** `client.ts` / `problem.ts` は多数のモジュールから import されるため、Vite の HMR で一部だけ更新されると `ApiError` のクラス実体が2つ存在する状態になる。`error instanceof ApiError` が false になり、`apiError` が null → `fieldErrors` が空 → `FormError` に `error.message` だけが出る、という経路をたどっていた。

**教訓**: `instanceof` によるエラー判別は HMR と相性が悪い。「コードは正しいのに動かない」という最も時間を溶かす形で現れる。挙動が説明できないときは、原因を追う前にまずフルリロードする。

### 4.3 `errors[]` の経路が #19 まで一度も動作確認されていなかった

`ApiError.fieldErrors` は #18 か #19 で実装されていたが、口座画面で実際に使われていたのは 409 を手動で `nameError` に流す経路だけで、`errors[]` を持つ 422 は検証項目に含まれていなかった。本タスクの取引フォームが初めての実使用。

4.2 の症状を「実装が壊れている」と読んだのは、この「動いた実績が無い」という認識があったため。結果的には実装は正しかった。

**教訓**: 実装したが一度も画面で確認していない経路は「未検証」であって「動く」ではない。ただし、それを理由に実装を疑いすぎると 4.2 のような遠回りをする。

### 4.4 `ProblemDetails` が手書き型と生成型で二重定義されていた

`api/problem.ts` の `FieldError` / `ProblemDetails` は `components["schemas"]` を参照せず手書きだった。`openapi.json` にも同名スキーマが存在するため、バックエンドがフィールド名を変えてもフロントは何事もなくコンパイルが通り、実行時に `undefined` を読むだけになる。

生成型に寄せたうえで `Partial<>` を被せた。

```ts
export type ProblemDetails = Partial<components["schemas"]["ProblemDetails"]>;
```

5xx は `trace_id` 以外を返さない設計なので、生成型をそのまま使うと「必ず存在する」と嘘をつくことになる。以前このファイルを手書きで optional に緩めた変更は正しかったが、それは*有無*の話であって*名前*まで手書きにする理由にはならない。`Partial` なら名前のズレはコンパイルエラーで落ち、有無の緩さは維持できる。

作業中、この差し替えを「ファイルごと置換」と指示しかけた。`ApiError` クラスが同じファイルにいるので、実行していれば口座画面が全滅していた。型定義の2行だけの置換で済ませた。

**教訓**: `problem.ts` のような小さいファイルでも、型とクラスが同居していることがある。差し替え指示の前に中身を確認する。

### 4.5 `AppError::field` のメッセージが #6 の分だけ英語で残っていた

検証で 422 のボディを見たところ、`detail` は「入力値が不正です」と日本語なのに `errors[].message` が `must not be in the future` だった。画面にそのまま英語が表示される。

`rg` で調査したところ、英語が残っていたのは `handler/assets.rs` と `handler/prices.rs` の11箇所だけ。`transactions.rs` / `analytics.rs` / `fx.rs` / `snapshots.rs` は最初から日本語で、#6（銘柄・価格）のときだけ日本語化を忘れていた。

テストがメッセージを文字列比較していないことを確認したうえで日本語化した（`rg 'must not|must be' tests/` でヒットしたのはすべて `expect()` のパニックメッセージだった）。

**教訓**: 表示に出る文言は、実装時ではなく画面に出したときに初めて確認される。ハンドラを追加したら `rg` で言語の混在を確認する習慣があってもよい。

### 4.6 TypeScript が早期 return の効果を追跡できない

`TransactionsPage` で `isPending` / `failed` の早期 return を書いたが、これは `accounts` / `assets` / `txs` という別々のクエリオブジェクトの `.isPending` / `.isError` から間接的に組み立てた条件なので、TypeScript は `txs.data` が非 undefined になったことを追跡できない。

ガード直後に `accountList` / `assetList` / `transactions` を `?? []` でデフォルト化して定義し、以降の描画をそちらに置き換えて解消した。既存の `useMemo` 内と同じパターン。

### 4.7 検証項目7が実装と噛み合っていなかった

「メモに空白だけを入れて登録 → メモ欄にエラー」を検証項目に入れたが、実際には登録が成功した。`buildCreateTransaction` が `note.trim()` が空ならキーごと省く実装なので、サーバーの `メモは空白のみにできません` に到達しない。

フロントの動作が正しく、検証項目のほうが誤り。空白だけのメモを「メモなし」と解釈するのは自然なので、実装は変えず検証項目を書き換えた。

**教訓**: 検証項目をサーバーのバリデーション一覧から機械的に起こすと、フロントが手前で防いでいるケースを「バグ」と誤認する。

## 5. 検証手順

### 5.1 銘柄（`/assets`）

1. `/assets` を開く → 空状態が出る
2. `7203` / `トヨタ自動車` / 株式 / JPY / 価格単位空欄 で作成 → 一覧に行が増え、価格単位が `1` になる
3. 資産クラスを投資信託にして空欄で作成 → 価格単位が `10000` になる（Network タブでリクエストボディに `price_unit` キーが**無い**ことを確認）
4. `7203` で再度作成 → コード欄の下に重複のエラー。上段に `FormError` が二重表示されない
5. 検索欄に `トヨ` → 1件に絞られる。`ZZZ` → 「一致する銘柄はありません」
6. 編集で名称だけ変更 → 一覧に反映
7. 何も変えずに保存 → 「変更された項目がありません。」が出て Network にリクエストが飛ばない
8. 既存の別銘柄と同じコードに変更 → コード欄にエラー

### 5.2 価格（`PriceDialog`）

1. 7203 の「価格」→ ダイアログが開き「まだ登録がありません」
2. 今日の日付・`2500` で登録 → 「1件を登録しました」、履歴に `manual` 付きで1行、入力欄が空に戻る
3. 同じ日付・`2600` で再登録 → 履歴が2行にならず既存行が `2600` に変わる（UPSERT）
4. 「日付を追加」で2行にし別々の日付で登録 → 「2件を登録しました」、履歴が降順
5. 未来日で登録 → 日付欄に「未来の日付は指定できません」
6. 価格 `-100` → 価格欄に「0以上の値を指定してください」
7. 空のまま登録 → 「価格を1件以上入力してください。」が出て Network にリクエストが飛ばない

### 5.3 取引（`/transactions`）

1. `/transactions` を開く → 空状態
2. 買付・数量100・単価2500・約定日を今日で登録 → リロードなしで一覧に行が増える
3. 口座名・銘柄名が UUID ではなく名前で表示される
4. 数量 `0` → 数量欄に「数量は正の数を指定してください」
5. 単価 `-1` → 単価欄に「価格は0以上を指定してください」
6. 保有100に対して売却200 → **フォーム上部**に「売却数量 200.00000000 が保有数量 100.00000000 を超えています」
7. メモに空白だけ → 空欄として扱われ登録が成功する（`note` キーが送信されない）
8. 買付→売却の順に登録し、買付のほうを削除 → 422 で「この取引を削除すると、以降の売却が保有数量を超えます」、削除ボタンが無効化
9. 売却のほうを削除 → 成功して一覧から消える
10. 口座・銘柄・期間のフィルタがそれぞれ効く。「条件をクリア」で全件に戻る
11. 開始日 > 終了日 → 422

### 5.4 ビルド

```bash
cd web && npx tsc -b --force && npm run lint
cd ../asset-log && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

## 6. 次タスクへの引き継ぎ

### 6.1 #21（保有一覧・評価損益）の前提

`GET /holdings` は `数量 × 価格 ÷ price_unit` で評価額を出す。**銘柄の `price_unit` が正しく登録されていることが前提**（4.1 参照）。#21 で数字が合わないときは、まず `/assets` の価格単位列を疑う。

価格が未登録の銘柄は `market_value` / `unrealized_pnl` が null で返る（#9 の設計）。`PriceDialog` から価格を入れておかないと、保有一覧が「—」だらけになる。

### 6.2 再利用できるもの

- `assets/labels.ts` の `assetClassLabels`、`transactions/labels.ts` の `tradeKindLabels` は保有一覧・分析画面でそのまま使える
- `currencyOptions` も同様。ただし置き場所は `features/assets/` のままでよいか要検討（口座画面からも使うなら共通の場所へ）
- `useDebounced` は `AssetsPage.tsx` にローカル定義している。他でも使うなら `lib/` へ移す
- ダイアログの `key` 方式（2.5）、`errors[]` の有無による出し分け（2.3）は以降の画面でも同じ形

### 6.3 口座画面に残った差分

本タスクで方針を新しくした箇所が、#19 の口座画面では旧いままになっている。まとめて揃える作業が必要。

| 項目 | 銘柄・取引（新） | 口座（旧） |
| --- | --- | --- |
| 編集ダイアログ | `key` によるアンマウント | ダミー空文字 id |
| 409 の文言 | `problem.detail` を使用 | フロントにハードコード |
| 削除 422 の文言 | `problem.detail` を使用 | フロントにハードコード |
| 通貨の入力 | `<select>` | 自由入力 |

### 6.4 未着手

- 銘柄・取引ともクライアント側の必須バリデーションが無い。空のまま送るとサーバーの 422 になる。動作としては正しいが、手前で止めるほうが親切
- 取引一覧の `limit` を指定していない。既定100件なので、それを超えると古い取引が見えない。ページングか「もっと読む」が必要
- `GET /assets/{id}` `GET /transactions/{id}` はフロントから未使用（#19 の `GET /accounts/{id}` と同じ状態）
- `TransactionsPage` の 422（開始日 > 終了日）は一覧全体のエラー画面に出る。フィルタ欄の直下に出すほうが親切
- `error_type()` を細分化すれば `status` 決め打ちをやめられる（2.2）。バックエンド側の変更として別途検討

## 7. 再現コマンド

### API 仕様の確認

```bash
cd ~/workspace/shisan-api/asset-log

# パスとメソッドの一覧
jq -r '.paths | to_entries[] | "\(.key): \(.value | keys | join(", "))"' docs/openapi.json

# 特定のスキーマ
jq '.components.schemas
    | with_entries(select(.key | test("Asset|Price|Transaction|Problem|FieldError")))' \
  docs/openapi.json
```

### サーバー既定値の確認（4.1）

```bash
TOKEN=$(curl -s -X POST http://localhost:8080/auth/login \
  -H 'content-type: application/json' \
  -d '{"email":"...","password":"..."}' | jq -r '.access_token')

# price_unit を指定しない → "10000" が返れば既定が効いている
curl -s -X POST http://localhost:8080/assets \
  -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"symbol":"TEST-MF","name":"テスト投信","asset_class":"mutual_fund"}' \
  | jq .price_unit
```

### エラーメッセージの言語混在を調べる（4.5）

```bash
cd ~/workspace/shisan-api

# 英語が残っていないか
rg -n 'must not|must be|is required|cannot be' asset-log/src/

# エラー生成箇所の一覧
rg -n 'AppError::field|AppError::unprocessable|AppError::BadRequest' asset-log/src/ -g '!tests'

# テストがメッセージを文字列比較していないか
rg -n 'must not|must be' asset-log/tests/
```