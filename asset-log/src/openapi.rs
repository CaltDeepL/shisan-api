use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::FieldError;

/// RFC 9457 準拠のエラーレスポンス（`application/problem+json`）
#[derive(Serialize, ToSchema)]
#[schema(as = ProblemDetails, title = "ProblemDetails")]
pub struct ProblemDetailsSchema {
    /// エラー種別の安定した識別子。クライアントはこの値で分岐できる
    #[serde(rename = "type")]
    #[schema(rename = "type", example = "/errors/not-found")]
    pub kind: String,

    /// エラー種別の短い説明
    #[schema(example = "Not Found")]
    pub title: String,

    /// HTTPステータスコード
    #[schema(example = 404, minimum = 100, maximum = 599)]
    pub status: u16,

    /// 人間向けの詳細メッセージ
    #[schema(example = "口座が見つかりません")]
    pub detail: String,

    /// 項目単位のエラー（422のみ。空の場合はフィールドごと省略される）
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<FieldError>,

    /// サーバーログと突き合わせるための識別子
    pub trace_id: Uuid,
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "asset-log API",
        version = "0.1.0",
        description = "NISA/iDeCo を含む資産・取引の管理API"
    ),
    servers(
        (url = "http://localhost:8080", description = "ローカル開発")
    ),
    tags(
        (name = "health", description = "ヘルスチェック"),
        (name = "auth", description = "ユーザー登録・ログイン"),
        (name = "accounts", description = "口座（特定/NISA/iDeCo等）の管理"),
        (name = "assets", description = "銘柄マスタと価格の登録"),
        (name = "fx", description = "為替レートの取得"),
        (name = "transactions", description = "売買取引の記録"),
        (name = "holdings", description = "保有ポジションと評価額"),
        (name = "analytics", description = "資産推移・アセットアロケーション"),
        (name = "import", description = "CSV一括取込"),
        (name = "snapshots", description = "日次スナップショットのバッチ実行"),
    ),
    components(
        schemas(ProblemDetailsSchema)
    ),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

pub struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);

        // ユーザーJWT（AuthUser 抽出子で保護しているエンドポイント用）
        components.add_security_scheme(
            "bearerAuth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );

        // バッチ専用トークン（JobAuth / SNAPSHOT_JOB_TOKEN）
        components.add_security_scheme(
            "jobToken",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some("バッチ実行用の固定トークン"))
                    .build(),
            ),
        );
    }
}
