# タスク#10 FxRateProvider + read-through キャッシュ

## 1. ゴールと完了条件

外貨建て資産の JPY 換算（タスク#11 以降）に必要な為替レートを、外部 API から取得しつつ
DB にキャッシュする層を実装する。外部 API が落ちてもサービス全体が止まらないことを担保する。

> **完了条件**: wiremock でテスト、障害時フォールバック

`tests/fx_test.rs` を 6 ケースで満たした。

| テスト | 検証内容 |
|---|---|
| `fetches_and_caches_rate` | 初回は外部を叩いて `fx_rates` に永続化、2回目は外部を叩かない（`expect(1)` で担保） |
| `stores_response_date_not_requested_date` | 土曜を要求 → 金曜の `rated_on` で保存される。要求日で保存しない |
| `falls_back_to_cache_on_5xx` | 5xx でキャッシュ値を返し `is_stale: true` |
| `falls_back_on_timeout` | タイムアウトでも同様 |
| `rejects_when_cache_too_old` | 方針超過のキャッシュしか無ければ 503 |
| `invalid_input_and_auth` | 不正な通貨コード・未来日で 422、必須パラメータ欠落で 400、未認証で 401 |

結果: `6 passed; 0 failed`（3.39s）

---

## 2. 成果物

| 区分 | ファイル |
|---|---|
| マイグレーション | `migrations/0005_fx_rates.sql` |
| domain | `src/domain/currency.rs`（`Currency` newtype） |
| provider | `src/provider/fx.rs`（trait・`FrankfurterClient`）<br>`src/provider/cached_fx.rs`（`CachedFxProvider`） |
| repository | `src/repository/fx_repo.rs` |
| handler | `src/handler/fx.rs`（`GET /fx/rates`） |
| error | `src/error.rs` に `ServiceUnavailable`（503）と `From<FxError>` を追加 |
| state | `AppState` に `Arc<dyn FxRateProvider>` |
| test | `tests/fx_test.rs`、`tests/common/mod.rs` に `test_app_with_fx` |

---

## 3. 設計判断

### 3-1. キャッシュはデコレータで挟む

`CachedFxProvider<P>` が `FxRateProvider` を実装し、内部に別の `FxRateProvider` を持つ。
service 層にキャッシュを置く案と比較して、次の点で優れる。

- 呼び出し側（handler・後続の analytics_service）はトレイト1本だけを見ればよい
- テストで「キャッシュあり／なし」を差し替えるのが `AppState` の組み立てだけで済む
- `FrankfurterClient` が HTTP のことだけを知っている状態を保てる

### 3-2. `rated_on` は応答の `date` を採用する

Frankfurter は ECB 由来のため、土日祝のレートが存在しない。非営業日を要求すると
**直前営業日のレートが返り、レスポンスの `date` がその実日付に書き換わる**。

要求日を `rated_on` として保存すると、存在しないはずの「土曜のレート」が DB に生まれる。
`stores_response_date_not_requested_date` はこれを回帰テストとして固定している。

### 3-3. `is_stale` は「外部障害の代替」に限定する

当初は「キャッシュから返したら stale」と考えたが、これは誤り。

| 状況 | `is_stale` |
|---|---|
| キャッシュヒット（過去日の確定値） | `false` |
| 土曜を要求して金曜の値が返る | `false`（ECB の正常動作） |
| 外部 API が 5xx／タイムアウトで過去値を代用 | `true` |

前者2つを `true` にすると、正常時に警告が鳴り続けて意味を失う。
`is_stale` は「本来取得すべき値が取れていない」というシグナルに限定する。

### 3-4. 過去日と当日で経路を分ける

```rust
let usable = c.rated_on == on || (is_historical && self.policy.accepts(c.rated_on, on));
```

- **過去日**: ECB の公表値は確定後に変わらないので、方針の範囲内なら外部に問い合わせない
- **当日**: まだ公表されていない可能性があるので、完全一致のキャッシュが無ければ必ず問い合わせる

この非対称性はテストを書く過程で初めて明示された。当初のテストは「過去日を要求して障害を起こす」
構成だったが、実装がキャッシュを返すため `is_stale` が立たず失敗した。**実装が正しく、
テストの前提が誤っていた**ケース。障害時フォールバックが実際に効いてほしいのは当日の
評価額計算なので、テストを当日要求に変更した。

### 3-5. 陳腐化の許容範囲は暦日と営業日の二軸

単一のしきい値では表現できない。

| 要求日 | キャッシュ | 暦日差 | 営業日差 | 判定 |
|---|---|:--:|:--:|---|
| 火 | 月 | 1 | 1 | 通る |
| 月 | 金 | 3 | 1 | 通る（週末を挟む） |
| 火 | 金（月が祝日） | 4 | 2 | 通る |
| 木 | 月 | 3 | 3 | **503**（平日3日放置は異常） |

`max_calendar_days = 4` かつ `max_business_days = 2` の AND 判定。
祝日カレンダー（TARGET 休業日）は持たない割り切りのため、「平日が2日続けて欠けた」
ケースだけは通してしまう。→ README の「意図的にやらなかったこと」候補。

### 3-6. 外部 API 起因の失敗は 500 ではなく 503

| 種別 | HTTP | 意味 |
|---|---|---|
| `UnsupportedPair` | 422 | 入力が悪い。リトライしても無駄 |
| `Unavailable` / `Transient` / `Upstream` | 503 | 依存先の問題。時間をおけば直る可能性がある |
| `Database` | 500 | こちらのバグ |

503 のみ、`detail` に内部情報を含まないまま「時間をおいて再度お試しください」という
**利用者が取れる行動**を返す。5xx で detail を伏せる既存ルールの例外だが、
実装の詳細を漏らしていないため問題ない。

### 3-7. リトライ対象の切り分け

`FxError::Transient` かどうかで、リトライとフォールバックの両方を分岐させている。

- リトライする: タイムアウト、接続断、5xx
- リトライしない: 404/422（対応外の通貨ペア）、不正 JSON、非正のレート

不正な通貨コードを3回投げても結果は変わらないので、指数バックオフの対象から外している。

### 3-8. `rate` は `normalize()` してから返す

`numeric(20,10)` から読むと `Decimal` のスケールが 10 になり、`147.2500000000` と
文字列化される。DB を往復したかどうかで API の表現が変わるのは望ましくないため、
handler で `normalize()` を通す。値は変わらず、末尾ゼロだけが落ちる。

---

## 4. 詰まった点

| 症状 | 原因 | 対処 |
|---|---|---|
| `cannot find trait FxRateProvider` 他、モジュール未解決が連続 | ファイルを作っても親の `mod.rs` に `pub mod 〜;` を書かないと Rust は認識しない | ファイル追加とモジュール宣言を必ずセットで行う |
| `cannot find provider in crate`（main.rs） | `lib.rs` と `main.rs` の2クレート構成のため、`main.rs` の `crate::` はバイナリ側を指す | `main.rs` からは `asset_log::` で参照する |
| `cannot find value fx`（main.rs） | `AppState` 構築が `fx` の生成より前にあった | 生成を先に移動。`db: pool` に所有権を渡すので `pool.clone()` が必要 |
| `?` が使えない（main.rs） | `run_server()` の戻り値が `()` | 起動時の設定ミスはプロセスを落とすのが正しいので `expect` にする |
| 3つの `match` が非網羅 | enum にバリアントを足すと `status()` / `error_type()` / `title()` の3箇所すべてに追加が要る | コンパイラが漏れを全部指摘してくれる。ワイルドカードを置いていない設計の利点 |

---

## 5. 環境変数（追加分）

```dotenv
FX_API_BASE_URL=https://api.frankfurter.dev/v1
FX_TIMEOUT_MS=3000
FX_MAX_CALENDAR_DAYS=4
FX_MAX_BUSINESS_DAYS=2
```

`.env.example` にも同じものを追加済み。旧 `api.frankfurter.app` も稼働しているが、
現行の公式ベース URL は `api.frankfurter.dev/v1`。

テストでは `test_app_with_fx` に wiremock の URL を渡す。タイムアウトは 500ms に短縮
（本番の 3000ms のままだと、タイムアウト検証のテストが毎回3秒待つため）。
`test_app` は到達不能な `http://127.0.0.1:1` を指しており、為替を使わない既存テストが
誤って本物の Frankfurter を叩かない安全弁になっている。

---

## 6. 次のタスクへの引き継ぎ

**タスク#11: `GET /analytics/asset-history`**（完了条件: 欠損日が補完される）

- JPY 換算が必要になるので `state.fx` を service から呼ぶ。`Arc<dyn FxRateProvider>` なので
  `analytics_service` にはトレイトオブジェクトを渡す形にできる
- 時系列で日ごとにレートを引くと、`fx_rates` に無い日は都度外部 API を叩くことになる。
  日付範囲をまとめて取得する `rates_range()` をトレイトに足すか、
  タスク#14 の日次スナップショットで先に埋めるか、着手時に決める
- `is_stale` をレスポンスにどう反映するか（1日でも stale なら全体に立てるか、日ごとに持つか）も
  タスク#11 の設計判断になる