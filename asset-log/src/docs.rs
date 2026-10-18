//! `/openapi.json` と Swagger UI（`/docs`）の配信。
//!
//! `utoipa-swagger-ui` は使わない。ビルド時に UI の zip を展開する build-dependency の
//! `zip` に修正版の無い advisory（RUSTSEC-2026-0367〜0369）が出たため、
//! CDN の `swagger-ui-dist` を読み込む静的 HTML に置き換えた（docs/task-34）。
//!
//! バージョンは完全一致で固定し、SRI（integrity）で配信内容を検証する。
//! 更新するときは `SWAGGER_UI_VERSION` と2つのハッシュを同時に書き換えること。
//! ハッシュの算出手順は docs/task-34 にある。

use std::sync::Arc;

use axum::{
    Router,
    http::header,
    response::{Html, IntoResponse},
    routing::get,
};
use utoipa::openapi::OpenApi;

const SWAGGER_UI_VERSION: &str = "5.33.1";
const SWAGGER_UI_CSS_SRI: &str =
    "sha384-Ov4/wv3j2bmct8cDc5X4ngJZohVPzEmc6uDPH8WeljUxO5vtoykvMEfbu9Vh6RaW";
const SWAGGER_UI_BUNDLE_SRI: &str =
    "sha384-ZPehFMQommnnuaZ4rpxgkgTT2DKFVp4hZC/7pLit+9Lek9T1YGSo23eHFbvNkXkw";

/// 仕様は起動時に1回だけシリアライズし、リクエストごとには組み立て直さない。
pub fn router(api: OpenApi) -> Router {
    let spec: Arc<str> = api
        .to_json()
        .expect("OpenAPI 仕様のシリアライズに失敗しました")
        .into();
    let html: Arc<str> = swagger_ui_html().into();

    Router::new()
        .route(
            "/openapi.json",
            get(move || {
                let spec = Arc::clone(&spec);
                async move {
                    (
                        [(header::CONTENT_TYPE, "application/json")],
                        spec.to_string(),
                    )
                }
            }),
        )
        // 旧実装（utoipa-swagger-ui）は /docs/ で配信していたので、既存のリンクのために両方で返す
        .route("/docs", get(docs_handler(Arc::clone(&html))))
        .route("/docs/", get(docs_handler(html)))
}

fn docs_handler(
    html: Arc<str>,
) -> impl Fn() -> std::future::Ready<axum::response::Response> + Clone + Send + Sync + 'static {
    move || std::future::ready(Html(html.to_string()).into_response())
}

fn swagger_ui_html() -> String {
    let base = format!("https://cdn.jsdelivr.net/npm/swagger-ui-dist@{SWAGGER_UI_VERSION}");
    format!(
        r#"<!doctype html>
<html lang="ja">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>asset-log API</title>
  <link rel="stylesheet" href="{base}/swagger-ui.css" integrity="{SWAGGER_UI_CSS_SRI}" crossorigin="anonymous">
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="{base}/swagger-ui-bundle.js" integrity="{SWAGGER_UI_BUNDLE_SRI}" crossorigin="anonymous"></script>
  <script>
    SwaggerUIBundle({{ url: '/openapi.json', dom_id: '#swagger-ui' }});
  </script>
</body>
</html>
"#
    )
}
