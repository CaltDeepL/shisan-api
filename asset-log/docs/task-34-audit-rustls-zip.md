# タスク #34: cargo audit の新規 advisory 対応（rustls / zip）

## 概要

`cargo audit` が、`main` を含むすべてのブランチで次の7件を検出して失敗するようになった。

| Crate | Version | ID | 修正版 | 経路 |
|---|---|---|---|---|
| rustls | 0.23.45 | RUSTSEC-2026-0346 / 0347 / 0348 / 0349 | 0.23.46 | reqwest / sqlx 経由 |
| zip | 8.6.0 | RUSTSEC-2026-0367 / 0368 / 0369 | 9.0.0 | `utoipa-swagger-ui 10.0.1` の build-dependency |

- **rustls**：lockfile の更新だけで解消（task #32 と同じ対応）
- **zip**：`utoipa-swagger-ui` は最新版でも zip 8.x を使っており、上流の更新を待つしかない。ignore ではなく、**`utoipa-swagger-ui` をやめて依存ごと外す**ことにした

## 1. rustls 0.23.45 → 0.23.46

```bash
cd asset-log
cargo update -p rustls
```

`Cargo.toml` とアプリケーションコードは変更していない。

## 2. zip：utoipa-swagger-ui を CDN + SRI の静的 HTML に置き換える

### 判断

| 案 | 内容 | 採否 |
|---|---|---|
| A | `.cargo/audit.toml` で ignore し、理由と見直しの条件をコメントに残す | 不採用 |
| B | `utoipa-swagger-ui` を外し、CDN の `swagger-ui-dist` を読む静的 HTML で配信する | **採用** |

zip の advisory はいずれも ZIP の展開時の問題で、展開するのは crate に同梱された Swagger UI だけなので、実際の攻撃経路は無い。それでも B を選んだ理由:

- ignore は「理由が消えたあとも残り続ける」。上流が直った時点で外す運用が必要になる
- Swagger UI を表示するだけのために、ビルド時に zip 展開・flate2・zopfli などを抱える必要がない（`Cargo.lock` から13クレート減った）
- chess プロジェクトでも同じ構成で運用実績がある

### 実装（`src/docs.rs`）

`SwaggerUi::new("/docs").url("/openapi.json", api)` が担っていた2つの役割を、自前の `docs::router(api)` に置き換えた。

| パス | 内容 |
|---|---|
| `/openapi.json` | 起動時に1回だけ `to_json()` した仕様を `application/json` で返す |
| `/docs` / `/docs/` | Swagger UI の HTML。旧実装は `/docs/` で配信していたため、既存リンクのために両方で返す |

chess からの改善点として、**CDN の読み込みをバージョン完全一致 + SRI** にした。

```html
<link rel="stylesheet"
      href="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5.33.1/swagger-ui.css"
      integrity="sha384-..." crossorigin="anonymous">
<script src="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5.33.1/swagger-ui-bundle.js"
        integrity="sha384-..." crossorigin="anonymous"></script>
```

- chess は `swagger-ui-dist@5`（major 指定）で、配信される中身が予告なく変わりうる
- SRI があると、CDN 側で内容が差し替えられた場合にブラウザが読み込みを拒否する
- バージョンは、公開から2週間以上たっている最新の 5.33.1（2026-10-01 公開）

### SRI ハッシュの算出と出所の確認

CDN のファイルが npm に公開されたパッケージと同一であることを確認してから、ハッシュを取った。

```bash
V=5.33.1
for f in swagger-ui.css swagger-ui-bundle.js; do
  curl -fsSL "https://cdn.jsdelivr.net/npm/swagger-ui-dist@$V/$f" -o "$f"
  echo "$f sha384-$(openssl dgst -sha384 -binary "$f" | openssl base64 -A)"
done

# npm のパッケージと一致すること（tarball の sha512 はレジストリの dist.integrity と一致）
curl -fsSL "https://registry.npmjs.org/swagger-ui-dist/-/swagger-ui-dist-$V.tgz" -o pkg.tgz
tar -xzf pkg.tgz
cmp swagger-ui.css package/swagger-ui.css
cmp swagger-ui-bundle.js package/swagger-ui-bundle.js
```

**バージョンを上げるときは、`SWAGGER_UI_VERSION` と2つのハッシュを必ず同時に更新する。** ずれていると、ブラウザが CSS / JS を拒否して Swagger UI が真っ白になる（API 自体は動く）。

### テスト

`openapi_test.rs` の Swagger UI のテストを強化した。旧テストは「200 またはリダイレクト」しか見ていなかった。

| テスト | 確認すること |
|---|---|
| `swagger_ui_is_served` | `/docs` と `/docs/` が 200 / `text/html`、`/openapi.json` を読む、CDN がバージョン固定で SRI が2つ付いている |
| `spec_is_served_as_json` | `/openapi.json` が 200 / `application/json` で、`openapi: 3.x` を含む |

共通の `request` ヘルパーは本文を JSON として読むので、HTML を確認するために `get_raw` を追加した。

## 検証

一時的な Postgres 17 コンテナで CI と同じ手順を実行した。

- [x] `cargo fmt --check` / `cargo clippy --all-targets -D warnings`
- [x] `cargo test --all-targets`：119件 pass（`spec_is_served_as_json` を追加）
- [x] `cargo sqlx prepare --check -- --all-targets`
- [x] `cargo audit`：脆弱性 0 件
- [x] テストが再生成する `docs/openapi.json` に差分なし（仕様の出力は変わっていない）
- [x] API をローカルで起動し、ブラウザで `/docs` を開いて Swagger UI が表示されること、コンソールにエラー（SRI 不一致を含む）が無いことを確認

## 変更範囲

```text
asset-log/Cargo.toml          utoipa-swagger-ui を削除
asset-log/Cargo.lock          rustls 0.23.46、zip ほか13クレートを削除
asset-log/src/docs.rs         新規（/openapi.json と /docs の配信）
asset-log/src/lib.rs          SwaggerUi → docs::router
asset-log/tests/openapi_test.rs
.github/dependabot.yml        utoipa グループのコメントから utoipa-swagger-ui を削除
README.md
```

## 残課題

- `swagger-ui-dist` は Dependabot の対象外（CDN の URL に直書き）。セキュリティ更新は自分で確認する必要がある。四半期に1回程度、バージョンと SRI をまとめて更新する
- Swagger UI は外部 CDN（jsDelivr）に依存する。CDN が落ちると `/docs` だけが表示できなくなる（API と `/openapi.json` は影響を受けない）
