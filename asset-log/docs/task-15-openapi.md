# タスク#15 OpenAPI（utoipa）

実装済みの全19エンドポイントをコードから OpenAPI 3.1 として生成し、Swagger UI で閲覧・実行できるようにした。

## 採用した構成

| 項目 | 選択 | 理由 |
|---|---|---|
| ルータ | `utoipa-axum` の `OpenApiRouter` に全面移行 | ルート登録と仕様生成が同じ `routes!()` にまとまり、パスの乖離が起きにくい |
| ドキュメント UI | Swagger UI（`/docs`） | Try it out で実際にリクエストを試せる |
| 公開範囲 | 本番でも公開 | ポートフォリオとして外部から参照できることを優先 |
| 仕様の配信 | `/openapi.json`（認証不要） | — |

`utoipa-swagger-ui` には `vendored` フィーチャーを付けている。これが無いとビルドスクリプトが Swagger UI の配布物をネットワーク取得しにいき、Docker の builder ステージが外部通信に依存する。

## 構造

```
src/openapi.rs
  ApiDoc                #[derive(OpenApi)]。info / servers / tags / components / modifiers
  SecurityAddon         Modify 実装。bearerAuth と jobToken を登録
  ProblemDetailsSchema  エラーレスポンスのドキュメント用型

src/lib.rs
  app()   OpenApiRouter に全ルートを .routes(routes!(...)) で登録し、
          split_for_parts() で (Router, OpenApi) に分割。
          SwaggerUi を merge して /docs と /openapi.json を配信
```

`app()` から素の `Router` は消え、全ルートが OpenAPI 管理下にある。今後エンドポイントを追加する際は `routes!()` に足すだけでよい。

## セキュリティスキーム

| 名前 | 対象 |
|---|---|
| `bearerAuth` | ユーザー JWT。`AuthUser` 抽出子で保護しているエンドポイント |
| `jobToken` | バッチ専用トークン（`SNAPSHOT_JOB_TOKEN`）。`POST /snapshots/run` のみ |

## 決めごと

### Decimal には必ず `#[schema(value_type = String)]` を書く

このプロジェクトの Decimal はすべて文字列でシリアライズされる。utoipa の `decimal` フィーチャーも文字列にマップするが、フィーチャーの挙動に依存せず明示する方針にした。`decimal_float` に切り替わったり、フィールドごとに `#[serde(with = ...)]` の有無が違ったりしても仕様が実態からずれない。

### エラーレスポンスは実際に起こりうるものだけ書く

「このエンドポイントは 404 を返しうる」という情報自体がドキュメントの価値なので、全パスに機械的に 500 を並べることはしない。

### `AppError` に `ToSchema` を付けてはいけない

`sqlx::Error` と `anyhow::Error` を内包しており、これらにスキーマは付けられない。レスポンスに現れるのは `ProblemDetails` の方なので、`AppError` 自体をドキュメント化する必要もない。`error.rs` で `ToSchema` が要るのは `FieldError` だけ。

### `/health` のレスポンス形状を変更した

`lib.rs` 内のローカル関数（`&'static str` で `"ok"` を返す）と `handler/health.rs`（`Json<HealthResponse>`）の2系統が存在していたため、後者に統一した。レスポンスは `"ok"` から `{"status":"ok"}` に変わっている。オブジェクトにしておくと、将来フィールドを追加する余地が残る。

## 既知の二重管理

`src/error.rs` の private な `ProblemDetails<'a>` が実際のレスポンス型、`src/openapi.rs` の `ProblemDetailsSchema` がドキュメント用型。`#[schema(as = ProblemDetails)]` で仕様上の名前を揃えている。

**`error.rs` のレスポンス形状を変えたら `openapi.rs` も直すこと。** ライフタイム付き・private のため `ToSchema` を直接付けられず、この構成になっている。

## エンドポイント固有の注意

`POST /import/transactions` の 422 は、ボディが `ProblemDetails` ではなく `ImportReport`（行ごとのエラー一覧）。他のエンドポイントとエラー形式が異なる唯一の箇所なので、`responses` の description にも明記している。

`POST /snapshots/run` はリクエストボディが省略可能（`Option<Json<RunRequest>>`）。utoipa で optional なボディを直接表現する方法が無いため、`request_body` の description に記載している。

## 検証

`tests/openapi_test.rs` に6ケース。

| テスト | 内容 |
|---|---|
| `spec_is_served_without_auth` | 認証なしで 200、`openapi` が `3.1.0` |
| `all_routes_are_documented` | 想定19パスがすべて存在し、**パス数が完全一致** |
| `security_schemes_are_defined` | 2スキームが定義され、`/snapshots/run` が `jobToken` を要求 |
| `error_schema_matches_problem_details` | `type`（`kind` ではない）を持ち、`errors` が必須でない |
| `decimal_fields_are_strings` | Decimal フィールドが `type: string` |
| `swagger_ui_is_served` | `/docs` が配信されている |

パス数を完全一致で検証しているのは意図的で、エンドポイントを追加すると `EXPECTED_PATHS` の更新を促してテストが落ちる。ドキュメント化漏れをここで拾う。

生成した仕様は `docs/openapi.json` にコミットしている。PR で契約変更が差分として見える。**タスク#15（GitHub Actions）で CI に `git diff --exit-code docs/openapi.json` を追加すること。**

## 移行中に踏んだ問題

| 症状 | 原因 |
|---|---|
| 起動時に `Overlapping method route` でパニック（3回） | `routes!()` に追加した際、`rest` 側の `.route(...)` を消し忘れ。コンパイルは通るので気づけない |
| 仕様に載らないパスがある | `#[utoipa::path]` の `path` と実際のルート登録が不一致（`/assets/{id}/prices` と `/prices/{asset_id}`）。**`rest` の該当行からパス文字列をコピーする**のが確実 |
| `expected ','` | `#[openapi(...)]` 内で末尾カンマ、または要素間のカンマ抜け |
| `ToSchema` の derive が通らない | DTO のフィールドが private。`pub` が必要 |

移行途中は「OpenAPI 管理下のルート」と「まだ移行していない素の Router」が併存するため、消し忘れが起きやすい。`rest` を空にして変数ごと削除したことで、この種の問題は構造的に解消した。

なお `PoolTimedOut` で起動に失敗した件は utoipa とは無関係で、`DATABASE_URL` が `localhost:5432` を指していたことが原因（コンテナ内からは `db:5432`）。

## 今後の課題

- `/docs` から `POST /snapshots/run` の存在とスキーム名が見える。トークン自体は漏れないが、気になるなら `tag = "internal"` で分離する手はある
- `openapi.json` の更新を CI で強制する（#16）