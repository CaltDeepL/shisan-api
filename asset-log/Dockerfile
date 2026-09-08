
# ---- builder ----
FROM rust:1.96-slim-bookworm AS builder
WORKDIR /app

# 【高速化】Cargoのレジストリとtargetディレクトリをキャッシュマウント
# ダミーのソースファイルを作って依存関係だけを先にビルド・キャッシュさせる
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
RUN mkdir src && echo "fn main() {}" > src/main.rs

# sqlx! マクロを使う場合はオフラインデータが必要（後述）
ENV SQLX_OFFLINE=true

# 依存関係をDockerレイヤーとしてキャッシュ
RUN cargo build --release

    # 本物のソースをコピー
COPY . .

RUN touch src/main.rs \
    && cargo build --release \
    && cp /app/target/release/asset-log /app/asset-log

# ---- runtime ----
FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
# ビルドステージからバイナリだけをコピー
# distroless:nonroot のデフォルトユーザー（nonroot:nonroot）に所有権を設定
COPY --from=builder --chown=nonroot:nonroot /app/asset-log .
COPY --from=builder --chown=nonroot:nonroot /app/migrations ./migrations

EXPOSE 8080
CMD ["./asset-log"]
# 【重要】シェルを介さず、バイナリを直接ヘルスチェックとして実行
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD ["./asset-log", "healthcheck"]
