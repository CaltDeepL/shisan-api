# タスク #25: CI/CD 運用の整備と Dependabot 対応

## 概要

chess-app との比較で見つかった運用面の穴を塞ぎ、Dependabot を導入して最初の major update に対応した。

| # | 項目 | 内容 |
|---|---|---|
| 1 | ブランチ保護 | main への直接 push を Ruleset で禁止 |
| 2 | Rust バージョンの一元化 | `rust-toolchain.toml` + Docker の ABI 不一致修正 |
| 3 | 依存の脆弱性監査 | `cargo audit` / `npm audit` を CI に追加 |
| 4 | Dependabot | Cargo / npm / GitHub Actions の3系統 |
| 5 | major update 対応 | `argon2 0.6` / `jsonwebtoken 11` |

順序は **1 → 2 → 3 → 4 → 5**。設定だけで効果が出るものから着手し、コードに手が入るものを後回しにした。

---

# 1. ブランチ保護

## 見つかった穴

README には「CI が green のときだけデプロイが走る」と書いてあり、`workflow_run` でその通りに実装されている。しかし **main への直接 push が可能だった**。

守られているものと守られていないものを整理すると:

| 守るもの | 仕組み | 状態 |
|---|---|---|
| CI が落ちた状態でデプロイされない | `workflow_run` | あった |
| CI を通さないコードが main に入らない | ブランチ保護 | **無かった** |

後者が無いと、レビューもテストも経ずに main が変わり、**CI が赤いまま放置される**。デプロイは止まるので本番は守られるが、main が壊れた状態になる。

## Classic ではなく Ruleset

| | Classic | Ruleset |
|---|---|---|
| 位置づけ | 旧方式 | 現行 |
| 一時的な無効化 | 削除するしかない | **Active / Disabled を切り替えられる** |

緊急時にルールを外したいとき、Classic だと削除して作り直すことになる。

## 設定

| 項目 | 値 |
|---|---|
| Enforcement | Active |
| Restrict deletions / Block force pushes | 有効 |
| Require a pull request | 有効 |
| └ Required approvals | **0** |
| Require status checks | `fmt / clippy / test` / `Frontend (React)` |

**Required approvals を 0 にするのが要点。** 1人開発では1以上にすると自分の PR を自分で承認できず、何もマージできなくなる。

## 効いていることの確認

```
GH013: Repository rule violations found for refs/heads/main.
- Changes must be made through a pull request.
- Required status check "fmt / clippy / test" is expected.
```

---

# 2. Rust バージョンの一元化と Docker の ABI 不一致

## rust-toolchain.toml

バージョンを `ci.yml`（`toolchain: "1.96.0"`）と `Dockerfile`（`FROM rust:1.96-slim`）の2箇所で指定していた。今は一致しているが、**片方だけ上げれば乖離する**。

chess-app では同じ構造で乖離が起き、依存の MSRV が上がったときに **テストも CI も緑のまま本番のビルドだけが落ちた**。

```toml
# asset-log/rust-toolchain.toml
[toolchain]
channel = "1.96.0"
components = ["rustfmt", "clippy"]
```

`ci.yml` から `dtolnay/rust-toolchain` のステップを削除した。ランナーには rustup が入っており、`cargo` の初回実行時にこのファイルを読んで自動で入る。**アクションを残してバージョンを書くと2箇所のままで、一元化の意味がなくなる。**

`components` はファイル側で指定しないと `cargo fmt` / `cargo clippy` が見つからない。

## Docker の COPY 位置

```dockerfile
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
```

**`cargo build` より前に置くことが必須。** chess-app では末尾に追加してしまい、ビルドの後にコピーされてまったく効いていなかった（ログの `builder 8/9` の後に `9/9` で気づいた）。

## builder / runtime の OS 世代不一致

chess-app で GLIBC のエラーが出たため、shisan-api も確認した。

```bash
docker run --rm rust:1.96-slim cat /etc/os-release | grep VERSION_CODENAME
# VERSION_CODENAME=trixie
```

| | ベース OS |
|---|---|
| builder: `rust:1.96-slim` | **trixie** |
| runtime: `gcr.io/distroless/cc-debian12` | **bookworm** |

**`-slim` は軽量化を意味するだけで、OS 世代を固定しない。** GLIBC は前方互換のみなので、ビルド環境のほうが新しいと実行環境でシンボルが解決できない。

```diff
- FROM rust:1.96-slim AS builder
+ FROM rust:1.96-slim-bookworm AS builder
```

**まだ壊れていなかっただけで、リスクは同じだった。** chess-app では実際に本番が5回連続で起動失敗していた。

## BuildKit の cache mount を外した

`-slim-bookworm` に変更したところ、次のエラーが出た。

```
error[E0463]: can't find crate for `asset_log`
error[E0463]: can't find crate for `sqlx`
```

原因は `--mount=type=cache,target=/app/target` と「ダミー src を消して本物をコピー」の組み合わせ。

| 方式 | 状態の持ち方 |
|---|---|
| BuildKit cache mount | レイヤーの外。**Docker が内容を追跡しない** |
| Docker レイヤーキャッシュ | レイヤーそのもの。COPY の内容が変われば無効化される |

cache mount は速いが、**Docker が「このキャッシュがどのソースに対応するか」を知らない**ため、ダミー src でビルドした成果物が本物のビルドに紛れ込む。レイヤーキャッシュに戻した。

## 確認手順

```bash
docker compose build --no-cache api
docker compose up -d --force-recreate api
curl -f http://localhost:8080/health
# {"status":"ok"}
```

**この順序でないと検証にならない。** ビルドが失敗していても、古いイメージが起動して `/health` が通ってしまう。

あわせて、使っていない `pkg-config` / `libssl-dev` を builder から削除した（`grep -n "openssl\|native-tls" Cargo.toml Cargo.lock` が空）。

---

# 3. 依存の脆弱性監査

## Dependabot との違い

| | 契機 | 対象 |
|---|---|---|
| Dependabot version updates | 新しいバージョンが出たとき | 直接依存 |
| Dependabot security updates | 脆弱性が公開されたとき | 直接・推移的依存 |
| `cargo audit`（CI） | **push / PR のたび** | `Cargo.lock` 全体 |

**3つ目の価値は「今この時点の lock ファイルが安全か」を毎回確認できること。** Dependabot は PR を出すだけなので、放置すれば脆弱なまま。

## 導入時に見つかった脆弱性

```
RUSTSEC-2026-0258  h2 0.4.15    → 0.4.16 以上
RUSTSEC-2026-0235  rkyv 0.7.46  → 0.8.17 以上
RUSTSEC-2024-0436  paste 1.0.15 unmaintained（警告）
                   chacha20 0.10.1 yanked（警告）
```

`cargo update` で3件とも解消した。

| crate | 対処 |
|---|---|
| `h2` | `cargo update -p h2` で 0.4.19 へ |
| `rkyv` | **依存グラフから参照されておらず、lock にだけ残っていた**。`cargo update` で削除 |
| `chacha20` | sqlx 経由。`cargo update` で 0.10.2 へ |

**`rkyv` は chess-app の `rsa` と同じ形だが結果が違った。** あちらは `sqlx-mysql` が実際に参照していたので消えず、こちらは参照が無かったので消えた。`cargo tree` が「nothing to print」でも、消えるとは限らない。

`paste` は `utoipa-axum` 経由で対処できないが、**unmaintained は cargo audit の既定では失敗扱いにならない**ため `audit.toml` は不要だった。

## CI への追加

```yaml
      - name: Security audit
        run: |
          command -v cargo-audit >/dev/null || cargo install cargo-audit --locked
          cargo audit
```

**`command -v` での存在確認が必須。** これを付けずに `cargo install` だけを書くと、キャッシュから復元されたときに失敗する。

```
error: binary `cargo-audit` already exists in destination
Add --force to overwrite
Error: Process completed with exit code 101
```

**初回は緑で、キャッシュが効き始めた2回目から壊れる**ため、導入直後には気づけなかった。Dependabot の PR が全部赤くなって発覚した。

## バイナリのキャッシュ

`cargo install` 系が CI 時間の大半を占めていた。

| ステップ | 前 | 後 |
|---|---|---|
| Security audit | 2m 45s | **4s** |
| Install sqlx-cli | 1m 4s | **0s** |
| Clippy | 57s | 6s |
| Test | 1m 41s | 56s |
| **合計** | **7m 21s** | **1m 59s** |

```yaml
      - name: Cache cargo binaries
        uses: actions/cache@v4
        with:
          path: ~/.cargo/bin
          key: cargo-bin-${{ runner.os }}-audit0.22.2-sqlx0.9.0
```

**キーにバージョンを含めるのが要点。** これがないと、バージョンを上げてもキャッシュが使われ続け、古いバイナリで検査することになる。

Clippy と Test も速くなっているのは `Swatinem/rust-cache` の効果で、ビルド成果物全体が効いている。

---

# 4. Dependabot

## 設定

```yaml
version: 2
updates:
  - package-ecosystem: "cargo"
    directory: "/asset-log"
    # ...
    groups:
      cargo-minor-patch:
        update-types: ["minor", "patch"]

  - package-ecosystem: "npm"
    directory: "/web"
    # ...
    groups:
      types:
        patterns: ["@types/*"]
      npm-minor-patch:
        update-types: ["minor", "patch"]

  - package-ecosystem: "github-actions"
    directory: "/"
```

**グループ化が要点。** Dependabot は既定で依存ごとに PR を立てるため、個別だと週に10本以上になる。数が多いと結局まとめて放置され、**「更新の自動化」を入れたのに更新が滞る**。

major はグループに含めない。1本の PR に破壊的変更が複数混ざると、落ちたときにどれが原因か分からない。

**chess-app と違い ESLint のグループ化は不要。** shisan-api は `oxlint` 単体で、プラグイン群が連動する問題がない。

## 初回の結果

8本の PR が来た。3系統すべてから提案されている。

| PR | 判断 |
|---|---|
| #35 typescript 6→7 | **上げた**（`oxlint` は TS バージョンに依存しない） |
| #33 @types/node | マージ |
| #32 tower-http 0.6→0.7 | マージ |
| #30 npm-minor-patch (4件) | マージ |
| #28 actions group (3件) | マージ |
| #34 argon2 0.5→0.6 | 対応（後述） |
| #29 jsonwebtoken 9→11 | 対応（後述） |
| #31 axum-extra 0.10→0.12 | 対応 |

**#35 は chess-app では見送った項目。** あちらは `typescript-eslint` が TS 7.0 に未対応で lint が起動しなくなるため見送ったが、shisan-api は `oxlint` を使っているので影響を受けない。**同じ更新でも、周辺のツール構成で判断が変わる。**

---

# 5. major update 対応（argon2 / jsonwebtoken）

## 変更内容

```toml
jsonwebtoken = { version = "11", default-features = false, features = ["aws_lc_rs"] }
argon2 = "0.6"
```

`argon2 0.6` では salt の生成を内部で行うため、`rand_core` の直接依存を削除した。

### Argon2

```diff
- let salt = SaltString::generate(&mut OsRng);
- Argon2::default().hash_password(plain.as_bytes(), &salt)
+ Argon2::default().hash_password(plain.as_bytes())
```

`PasswordHash` は `password_hash::phc::PasswordHash` に移動している。

**salt の生成を呼び出し側に書かせない設計への変更**で、salt の使い回しや乱数源の誤りといった事故が構造的に起きなくなる。

### jsonwebtoken

v11 では crypto backend を明示しないと、**compile は通っても JWT 発行時に panic する**。

当初 `rust_crypto` を選択したが、CI の `cargo audit` で `rsa 0.9.10` の RUSTSEC-2023-0071（修正版なし）が検出された。HS256 しか使っておらず RSA JWT は使用していないが、**修正版のない advisory を allowlist して残すより、別 backend に切り替える**方針を採った。

```diff
- features = ["rust_crypto"]
+ features = ["aws_lc_rs"]
```

**chess-app では逆の判断をした。** あちらは `aws_lc_rs` の C++ ビルドを避けて `rust_crypto` を選び、`rsa` は `sqlx-mysql` 経由でも入るため `audit.toml` で ignore した。shisan-api は `rsa` の経路が `rust_crypto` だけだったので、backend を変えれば消える。**同じ問題でも、依存グラフの形で最善手が変わる。**

## 追加したテスト

`tests/auth_test.rs` に4件。

| テスト | 確認内容 |
|---|---|
| `password_hash_and_verify_round_trip` | 新規 hash が検証でき、誤パスワードを拒否する |
| `password_verifies_legacy_phc_hash` | **旧 Argon2 PHC 文字列を 0.6 でも検証できる** |
| `password_rejects_malformed_hash` | 不正な PHC 文字列を拒否する |
| `jwt_issue_and_verify_round_trip` | JWT 発行・検証が panic せず通る |

**2つ目が最重要。** ハッシュ形式が変わると既存ユーザー全員がログインできなくなる。新規 hash の round trip だけでは、**生成側も検証側も同じ実装になるため互換性を検証できない**。固定の PHC fixture を埋め込んでいる。

4つ目は `cargo check` では検出できない runtime の問題を守る。**依存更新では compile green だけでなく、対象ライブラリの主要 runtime path まで実際に通す必要がある。**

---

# つまずいた点

## 依存を消した状態で `cargo update -p` を実行した

`Cargo.toml` から `argon2` / `jsonwebtoken` が抜けた状態で `cargo update -p argon2 --precise 0.6.0` を実行したところ、Cargo は「現在の dependency graph では不要」と判断して lock から削除した。

```
Removing argon2 v0.5.3
Removing jsonwebtoken v9.3.1
```

**`cargo update -p` は dependency を追加するコマンドではなく、既に graph に存在する package の解決を更新するコマンド。**

## argon2 0.5 / 0.6 のコードを混在させた

import だけ 0.6 向けに変えて、旧 API の salt 生成を残してしまった。

```
error[E0432]: unresolved import `rand_core`
error[E0433]: cannot find type `SaltString` in this scope
```

**major update では compile error が出た行だけを逐次直すのではなく、旧 API に依存している一連の処理をまとめて置き換える。**

## CI のワークフローが2系統あることに気づかなかった

`ci.yml` だけを見て「フロントエンドは CI で検証されていない」と判断し、`frontend` ジョブを追加した。実際には `ci-web.yml` が最初から存在していた。

```
ci-web.yml（CI (web)）→ deploy-web.yml → Render Static Site
ci.yml（CI）          → deploy.yml     → Render Web Service
```

**`.github/workflows/` の中身を確認せずに判断した。** 結果として同じ検査が二重に走っている。

## `cargo install` がキャッシュと衝突した

前述のとおり、**初回は緑で2回目から壊れる**タイプの不具合。導入した PR では気づけず、Dependabot の PR が全部赤くなって発覚した。

**キャッシュを入れたら、キャッシュが効く状態で1回試す。** 初回の実行はキャッシュ保存であって、動作確認にはならない。

---

# 検証項目

| # | 確認 | 結果 |
|---|---|---|
| 1 | main への直接 push が拒否される | OK |
| 2 | `rustup show` に rust-toolchain.toml が表示される | OK |
| 3 | builder / runtime の Debian 世代が一致 | OK（bookworm） |
| 4 | `docker compose build --no-cache` → `/health` | OK |
| 5 | `cargo audit` exit code 0 | OK |
| 6 | `npm audit` 0 vulnerabilities | OK |
| 7 | CI 時間 7m21s → 1m59s | OK |
| 8 | Dependabot 3系統から PR | OK |
| 9 | 旧 Argon2 PHC を 0.6 で検証できる | OK |
| 10 | JWT issue → verify が panic せず通る | OK |
| 11 | `cargo test --all-targets` | OK（107 tests） |
| 12 | main merge 後の CI | OK |

---

# 残課題

## 運用

| 項目 | 内容 |
|---|---|
| **ワークフローの重複** | `ci.yml` の frontend ジョブと `ci-web.yml` が同じ検査をしている。**`deploy-web.yml` は `CI (web)` を待っている**ため、検査とデプロイのトリガーがずれている。統合が必要 |
| `paths` フィルタとブランチ保護 | `paths` があると起動しないジョブのステータスチェックが待ち状態になる。フィルタを外すか、スキップ時も成功を返す仕組みが要る |
| Require branches to be up to date | Dependabot の PR で毎回 `Update branch` が必要になる。PR が多いと手間 |
| デプロイ失敗の検知 | chess-app では本番が5回連続で壊れていたのに気づけなかった。Render の Webhook 等で通知したい |
| `sqlx-cli` のバージョン | `ci.yml` に直書き。`sqlx` 本体を上げたら手で合わせる（キャッシュキーにも含まれる） |

## 認証仕様

| 項目 | 内容 |
|---|---|
| JWT expiry | 有効期限切れ token の拒否テスト |
| JWT tamper | payload 改ざん token の拒否 |
| wrong signing secret | 別 secret で署名された token の拒否 |
| パスワード要件 | **文字数で数える**（`str::len()` はバイト数）、上限（無いと Argon2 が DoS の入口）、denylist |
| 起動時 migration | `sqlx::migrate!()` を起動時に実行。`#[sqlx::test]` は毎回専用DBに全マイグレーションを当てるため、**テストは緑のままローカル・本番だけ壊れる** |
| OpenAPI error contract | 全 4xx/5xx が `ProblemDetails` を返すか自動検証 |
| フロント 401 | API wrapper で一元検知し、logout / redirect を統一 |

今回の `auth_test.rs` は Dependabot major update の破壊的変更を防ぐための最小 regression。認証仕様そのものの強化は別タスクに分離する。