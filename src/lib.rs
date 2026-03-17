//! axum78 - 基于 axum 的快速 Web API 框架
//!
//! 参考 koa78-base78 的 4 级路由架构: /:apisys/:apimicro/:apiobj/:apifun
//!
//! 请求格式: UpInfo (JSON)
//! 响应格式: Response (JSON)

pub mod context;
pub mod response;
pub mod base_api;
pub mod router;
pub mod server;

pub mod apitest;
pub mod apisvc;
pub mod apigame;

pub use context::{UpInfo, RequestBody, Context, VerifyResult, verify_sid, verify_sid_web, get_lovers_state, LoversDataState, AuthConfig, get_auth_config, sid_auth_middleware};
pub use response::{ApiResponse, ApiError};
pub use base_api::{BaseApi, TableConfig};
pub use router::{ApiRouter, ApiRouter78, Controller78};
pub use server::Server;

pub use async_trait::async_trait;

pub use axum::{
    Router,
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use axum::{
    body::Bytes,
    extract::Path as AxumPath,
    http::{header, StatusCode as AxumStatusCode},
    response::IntoResponse,
    routing::any,
    Router as AxumRouter,
    middleware,
};
use std::sync::Arc;
use base::Response;
use tower_http::cors::{CorsLayer, Any};

#[derive(Clone)]
pub struct AppState;

impl AppState {
    pub fn new() -> Self {
        Self
    }
}

pub fn create_router() -> AxumRouter<AppState> {
    let state = Arc::new(AppState::new());
    
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST, axum::http::Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);
    
    AxumRouter::new()
        .route("/:apisys/:apimicro/:apiobj/:apifun", any(api_handler))
        .layer(middleware::from_fn(sid_auth_middleware))
        .route("/health", axum::routing::get(health_handler))
        .with_state(state)
        .layer(cors)
}

async fn health_handler() -> impl IntoResponse {
    (AxumStatusCode::OK, "OK")
}

async fn api_handler(
    AxumPath((apisys, apimicro, apiobj, apifun)): AxumPath<(String, String, String, String)>,
    axum::extract::Extension(verify_result): axum::extract::Extension<VerifyResult>,
    axum::extract::Extension(up): axum::extract::Extension<UpInfo>,
) -> impl IntoResponse {
    let apisys_lower = apisys.to_lowercase();
    let apimicro_lower = apimicro.to_lowercase();
    let apifun_lower = apifun.to_lowercase();

    let (status, resp_bytes) = match (apisys_lower.as_str(), apimicro_lower.as_str(), apiobj.as_str()) {
        ("apitest", "testmenu", "testtb") => {
            apitest::testmenu::testtb::handle(&apifun_lower, up, &verify_result).await
        }
        ("apisvc", "backsvc", "synclog") => {
            apisvc::backsvc::synclog::handle(&apifun_lower, up).await
        }
        ("apigame", "mock", "game_state") => {
            apigame::mock::game_state::handle(&apifun_lower, up).await
        }
        _ => {
            let resp = Response::fail(&format!("API not found: {}/{}/{}/{}", apisys, apimicro, apiobj, apifun), 404);
            (AxumStatusCode::NOT_FOUND, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    };

    (status, [(header::CONTENT_TYPE, "application/json")], resp_bytes)
}
