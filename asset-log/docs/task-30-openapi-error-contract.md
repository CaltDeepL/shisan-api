# タスク #30: OpenAPI error contract の自動検証

## 概要

既存の OpenAPI test に、各 operation の `responses` を走査して documented 4xx / 5xx が意図した error schema を参照しているかを検証するテストを追加する。

## 基本ルール

原則として 4xx / 5xx は次を参照する。

```text
#/components/schemas/ProblemDetails
```

## CSV import 422 の扱い

`POST /import/transactions` の 422 は既存仕様として `ImportReport` を返す。

```text
total_rows
to_insert
to_skip_duplicate
errors[]
```

これは CSV 行番号と取込計画を UI へ返す domain response なので、現時点では `ProblemDetails` へ無理に変換しない。

例外をコード上で明示する。

```rust
const ERROR_RESPONSE_EXCEPTIONS: &[(&str, &str, &str, &str)] = &[(
    "/import/transactions",
    "post",
    "422",
    "#/components/schemas/ImportReport",
)];
```

例外指定そのものが spec から消えた場合もテストを失敗させ、古い allowlist が放置されないようにする。

## テスト方式

```text
paths
  ↓
HTTP method
  ↓
responses
  ↓
4xx / 5xx
  ↓
schema.$ref を検証
```

原則 `ProblemDetails`、明示した例外だけ `ImportReport` を許可する。

## 検証

```bash
cd asset-log
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings

cd ../web
npm run gen:api
git diff --exit-code -- src/api/schema.d.ts
npm run typecheck
npm run lint
npm run build
```

## 残課題

CSV import 422 を将来 RFC 9457 に完全統一する場合は、`ProblemDetails` の extension member として ImportReport 相当を持たせるか、422 の API 契約自体を再設計する。