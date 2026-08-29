# タスク#16 残課題の消化

タスク#16(デプロイ・GitHub Actions)完了時に先送りにした6件を消化した記録。
ロードマップ全16タスクの外側の作業であり、新規機能の追加は含まない。

---

## ゴールと完了条件

**ゴール**: task-16 のメモに列挙した残課題6件について、対応するか「対応不要」と判断するかを決着させ、宙に浮いた項目をゼロにする。

**完了条件**

- [x] 6件すべてに結論が出ている(対応済み / 対応不要 のいずれか)
- [x] コード変更を伴うものは `cargo test` と `clippy --all-targets` が green
- [x] CI と Deploy が本番で green になっている
- [x] 「対応不要」と判断したものは、その理由が本メモに残っている

**結果**: 全93テスト green、CI #10 / Deploy #3 ともに成功。

---

## 各課題の結論

| # | 項目 | 結論 |
|---|---|---|
| 1 | reqwest の `default-features` | 対応済みだったと判明 |
| 2 | reqwest の `blocking` | 対応済みだったと判明 |
| 3 | 日次実行が毎回7日分 | 直近3日分に変更 |
| 4 | 起動時マイグレーション | **対応不要と判断** |
| 5 | CI に `cargo sqlx prepare --check` | 追加 |
| 6 | `actions/checkout@v4` | v5 に更新 |

副産物として cargo-machete による未使用依存の調査を行ったが、削除対象はゼロだった(後述)。

---

## 1・2. reqwest の features — 課題の前提が既に崩れていた

メモには「hyper-tls が余分にビルドされている」「blocking は削除候補」とあったが、着手前に現物を確認したところ**どちらも既に解消済み**だった。

```
reqwest v0.12.28 __rustls,__rustls-ring,__tls,h2,http2,json,
                 rustls-tls,rustls-tls-webpki-roots,rustls-tls-webpki-roots-no-provider
```

- `default-features = false` が適用済み。`hyper-tls` / `openssl-sys` はツリーに存在しない
- `blocking` も無効。healthcheck は非同期実装に書き換え済み
- TLS は rustls 0.23.43 + ring 0.17.14 の一本。reqwest と sqlx が同一の rustls / 同一プロバイダを共有しており、二重プロバイダ問題も起きていない
- ルート証明書は `webpki-roots`(バンドル)。distroless に ca-certificates が無くても動く

**この2件は記録が実態より古かっただけで、作業は不要だった。**

なお `cargo tree -d` に `rustls` や `ring` が現れるが、これらが重複しているのではなく、重複している別クレート(`getrandom` 0.2/0.4、`rand` 0.8/0.10、`digest` 0.10/0.11、`sha2` 0.10/0.11、`syn` 2/3)の**逆依存として表示されている**だけ。上流のバージョン差に起因するもので、自力では解消できないし実害もない。

---

## 3. 日次スナップショットの実行範囲

### 変更内容

`POST /snapshots/run` にボディを付け、JST 基準で算出した `from` / `to` を渡すようにした。既定の範囲は**直近3日分**(開始日 = 2日前、終了日 = 当日)。

```yaml
TO=$(TZ=Asia/Tokyo date +%F)
FROM=$(TZ=Asia/Tokyo date -d "$LOOKBACK_DAYS days ago" +%F)
```

`workflow_dispatch` に `from` / `to` の入力を追加し、過去取引を追加したときの手動バックフィルにも使えるようにした。遡り日数は `env.LOOKBACK_DAYS` の1箇所に集約。なお実装時に、`FROM=` の代入と `echo` が1行に結合していたバグを踏んでいる(詳細は「つまずいた点と教訓」6)。CI では検証できないため、変更後は必ず手動実行してログを確認する。

```bash
gh workflow run "Daily Snapshot"
gh run watch
```

`range:` 行に JST の日付が2つとも出ていれば正常。片方でも空なら代入が効いていない。

### 判断の根拠

**なぜ1日ではなく3日か。** メモには「1日分に絞る」と書いていたが、厳密に1日にすると実行が1回落ちたその日は永久に未計算のまま残る。asset-history 側がフォールバックで再計算するため壊れはしないが、キャッシュとしての意味が失われる。重複幅を持たせれば翌日の実行が自動で穴を埋める。

重複を許容できる理由は3つ。

- GitHub の `schedule` は数十分の遅延やスキップが起きうる
- 前日分の価格を後から登録した場合、翌日の実行で拾い直せる
- Frankfurter は営業日ベースで、直近日のレートが翌日に確定することがある

処理は冪等なので重複実行に副作用は無く、3日分でも実行時間は誤差の範囲。

**日付基準のずれ。** cron が `0 22 * * *`(UTC) なので、実行時点の UTC 日付は JST より1日前になる。サーバ側が `Utc::now().date_naive()` で既定範囲を決めていたため、これまでの実行は JST 感覚より1日ずれた範囲を計算していた。`TZ=Asia/Tokyo` で明示的に渡すことでここが揃った。

---

## 4. 起動時マイグレーション — 対応不要と判断

**結論: 移設しない。** 当初の「単一インスタンス前提だからスケール時に危ない」という懸念は、実際には成立していなかった。

理由:

- sqlx の Migrator は実行時に `pg_advisory_lock` を取得する。複数インスタンスが同時起動しても多重適用は起きない
- Render の Pre-Deploy Command は有料インスタンス限定の可能性があり、無料枠では選択肢にならない
- 分離するなら GitHub Actions 側(Deploy Hook を叩く前にマイグレーション実行)が確実だが、その場合「新コードが古いスキーマで起動する」瞬間が生まれ、後方互換なマイグレーションを書く規律とセットになる

得られる安全性に対して運用上の制約が見合わない。現構成のままとする。

**再検討する条件**: インスタンス数を2以上にする、またはゼロダウンタイムデプロイを要求するようになったとき。

---

## 5. CI に `cargo sqlx prepare --check`

`.sqlx` の陳腐化を検知できるようにした。Test ステップの後ろに3つ追加。

```yaml
      - name: Install sqlx-cli
        run: cargo install sqlx-cli --version 0.9.0 --no-default-features --features rustls,postgres --locked

      - name: Run migrations
        run: cargo sqlx migrate run

      - name: Check .sqlx is up to date
        env:
          SQLX_OFFLINE: "false"
        run: cargo sqlx prepare --check -- --all-targets
```

3点の注意が必要だった。

- ジョブ全体に `SQLX_OFFLINE: "true"` がかかっているため、このステップだけ `"false"` で上書きする
- **CI にマイグレーション適用ステップが存在しなかった。** テストは `#[sqlx::test(migrations = "./migrations")]` が各テスト用の独立DBに自前で適用するため、サービスコンテナの `assetlog` DB は空のまま。`prepare` は実DBのスキーマを見るので、先に適用しないとテーブル不在で落ちる
- sqlx-cli のバージョンをローカル(0.9.0)と揃える。ズレると `.sqlx` のフォーマット差分で誤検知する

既存の `.sqlx` は `--all-targets` なしで生成されていた可能性を疑ったが、ローカルで `cargo sqlx prepare --check -- --all-targets` を実行したところ差分なしで通過したため、再生成は不要だった。

`cargo install` は初回2〜3分かかるが、`Swatinem/rust-cache@v2` が `~/.cargo/bin` をキャッシュするため2回目以降は復元される。

---

## 6. actions/checkout@v5

`grep -rn "uses:" .github/workflows/` で棚卸ししたところ、`uses:` を持つのは ci.yml の3行のみだった。deploy.yml と snapshot.yml は curl 実行だけで checkout を使っていない。

- `actions/checkout@v4` → `@v5`(Node 24 ランタイム。GitHub ホストランナーなら追加対応不要)
- `dtolnay/rust-toolchain@master` — 変更なし
- `Swatinem/rust-cache@v2` — 変更なし

---

## 副産物: cargo-machete による未使用依存調査

`cargo machete` が4件(`rand` / `tokio-cron-scheduler` / `tower-http` / `validator`)を未使用と報告したが、**最終的に削除対象はゼロ**だった。

- `--with-metadata` を付けると `tokio-cron-scheduler` / `tower-http` / `validator` の3件が解消。derive マクロや Layer 経由の利用を静的な文字列検索で追えていなかった
- 残った `rand` を削除したところビルドが落ちた

```
error[E0432]: unresolved import `argon2::password_hash::rand_core::OsRng`
note: found an item that was configured out
49 | #[cfg(feature = "getrandom")] pub use os::OsRng;
```

`argon2::password_hash::rand_core` の実体は `rand_core 0.6.4` で、`OsRng` は `getrandom` フィーチャーが有効なときだけ公開される。argon2 側にそのフィーチャーを有効化する経路は無く、**`rand 0.8` が依存として `rand_core/getrandom` を引いていたことで暗黙に成立していた**。Cargo はワークスペース全体でフィーチャーを合成するため、ソース上どこからも `rand::` を呼んでいなくても、置いてあるだけで効いていた。

意図が読めない状態を解消するため、フィーチャー供給を直接依存として明示する形に変更した。

```toml
rand_core = { version = "0.6", features = ["getrandom"] }
```

```rust
use rand_core::OsRng;
```

`use` でソースに名前が出るため machete も誤検出しなくなり、「なぜこの依存があるのか」がコードから読める。バージョンは argon2 0.5 が使うものと一致させる必要がある(`0.9` を入れると型が別物になり `SaltString::generate` に渡せない)。

---

## つまずいた点と教訓

### 1. 記録した課題の前提が実態とずれていた

6件のうち2件は、着手してみると既に対応済みだった。メモを書いた時点の想定がそのまま残っていたもの。

**教訓**: 先送りにした課題は、着手前にまず現物を確認する。`cargo tree` 一発で済む確認を怠ると、不要な作業に時間を使う。

### 2. cargo-machete は「フィーチャー供給専用の依存」を検出できない

ソース中のクレート名を文字列として探す実装のため、derive マクロ経由の利用と、フィーチャー統合のためだけに存在する依存を落とす。後者は原理的に検出不可能。

**教訓**: `--with-metadata` を付けたうえで、削除は必ず1件ずつ検証する。`cargo build` だけでは不十分で、テスト専用の依存は `clippy --all-targets` まで走らせて初めて落ちる。

```bash
cargo build --release && cargo test && cargo clippy --all-targets -- -D warnings
```

### 3. `cargo tree -i` は同一クレートの複数バージョン同居で ambiguous になる

`rand_core` は 0.6.4(argon2 系)と 0.10.1(sqlx 系)が同居しているため、バージョン指定が必要。

```bash
cargo tree -e features -i rand_core@0.6.4
```

### 4. `Content-Type` の要否がボディの有無で逆転する

`/snapshots/run` のボディは `Option<Json<RunRequest>>` で任意。

- ボディ**無し**で `Content-Type: application/json` を付ける → ボディ空で400(タスク#16で踏んだ)
- ボディ**有り**で `Content-Type` を付けない → ボディが無視され既定の7日分に戻る

今回はボディを送るので付ける。前回の教訓をそのまま適用すると逆に壊れる。

### 5. `workflow_dispatch` の `description` は静的文字列

変数展開されないため、遡り日数を変えても説明文は追随しない。実際に `'2 days ago'` に対して説明文が「3日前」とズレた(開始日=2日前 / 範囲=3日分 を取り違えたもの)。

**教訓**: 日数と範囲は別物なので、説明文には両方書く。数値は `env` の1箇所に集約して対応関係を追いやすくする。

```yaml
description: '開始日 (YYYY-MM-DD、空なら2日前＝直近3日分)'
```
### 6. `VAR=value command` はコマンド固有の環境変数になる

snapshot.yml で、行の結合により以下の形になっていた。

```bash
FROM=$(TZ=Asia/Tokyo date -d "$LOOKBACK_DAYS days ago" +%F) echo "range: $FROM .. $TO"
```

これは代入文ではなく、`echo` の実行環境にだけ `FROM` を設定する構文。シェル本体の `$FROM` は空のままとなり、後続の `curl` に `{"from":"","to":"..."}` が飛ぶ。

さらに悪いことに、`echo` の引数は代入が反映される**前**に展開されるため、ログにも空文字が出る。「ログの `range:` 行で範囲を確認する」という検証手段そのものが同時に潰れており、失敗が表面化しない。

**教訓**: シェルの構文差でサイレントに壊れるパターンは、タスク#16 で踏んだ zsh の `read -p` 非互換と同じカテゴリ。ワークフローのシェルスクリプトは、変数が実際に渡っているかをログで確認できる形にしたうえで、`workflow_dispatch` で一度手動実行して目視する。

### 7. Cargo.lock の変更は必ずコミットに含める

依存整理で `Cargo.lock` が439行削減された。これが欠けると Docker ビルドが古い依存で走る。

---

## 次への引き継ぎ

残課題はゼロ。次の候補は task-16 のメモに記載した追加機能。

| 優先 | 項目 | 備考 |
|---|---|---|
| 1 | XIRR(金額加重収益率) | asset-tracker-design.md への追補として扱う |
| 2 | Google ログイン(OIDC) | 自前の register/login + JWT の上に乗せる |

新たに認識した検討事項:

- **サーバ側の既定日付が UTC 基準**。`/snapshots/run` をボディ無しで叩いた場合や、他のエンドポイントで `as_of` を省略した場合も同じずれが起きうる。JST 固定にするかタイムゾーンを設定可能にするかは未決
- 4(起動時マイグレーション)はインスタンス数を増やす際に再検討する

---

## 再現コマンド

```bash
# 依存ツリーの確認
cd ~/workspace/shisan-api/asset-log
cargo tree -p reqwest -f "{p} {f}" --depth 0
cargo tree -i hyper-tls          # 「did not match any packages」が正解
cargo tree -i openssl-sys        # 同上
cargo tree -e features -i rand_core@0.6.4

# 未使用依存の調査
cargo install cargo-machete
cargo machete --with-metadata

# 通しの検証
cargo build --release && cargo test && cargo clippy --all-targets -- -D warnings

# .sqlx の陳腐化チェック(ローカル)
# --all-targets は内部の cargo check へ渡す引数なので `--` の区切りが必須
cargo sqlx prepare --check -- --all-targets
# 差分が出た場合の再生成
cargo sqlx prepare -- --all-targets && git status .sqlx/

# ワークフローのアクション棚卸し
cd ~/workspace/shisan-api
grep -rn "uses:" .github/workflows/

# 日次ジョブの手動実行(範囲を指定してバックフィル)
gh workflow run "Daily Snapshot" -f from=2026-08-01 -f to=2026-08-29
```